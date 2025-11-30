#!/usr/bin/env node

/**
 * Build SuperNOVA Base Ontology from WordNet
 *
 * This script:
 * 1. Parses WordNet RDF/Turtle file
 * 2. Converts synsets → owl:Class
 * 3. Maps semantic relations to RDF properties
 * 4. Imports triples into SQLite database
 *
 * Output: core-ontology/supernova.db (ready for git commit)
 */

import fs from 'fs';
import path from 'path';
import { fileURLToPath } from 'url';
import Database from 'better-sqlite3';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const PROJECT_ROOT = path.resolve(__dirname, '..');
const WORDNET_PATH = path.join(PROJECT_ROOT, 'core-ontology/english-wordnet-2024.ttl');
const DB_PATH = path.join(PROJECT_ROOT, 'core-ontology/supernova.db');

// Transaction counter
let txCounter = 1;
const ORIGIN = 'core';
const timestamp = Date.now();

/**
 * Parse WordNet Turtle file (simplified parser for our needs)
 * For production, we'd use a proper RDF library like N3.js
 */
function parseWordNetSynsets(filePath, limit = 10) {
  console.log(`📖 Reading WordNet from ${filePath}...`);

  if (!fs.existsSync(filePath)) {
    console.error(`❌ WordNet file not found. Run: npm run update:wordnet`);
    process.exit(1);
  }

  const content = fs.readFileSync(filePath, 'utf-8');
  const lines = content.split('\n');

  const synsets = [];
  let currentSynset = null;
  let inSynset = false;

  for (const line of lines) {
    const trimmed = line.trim();

    // Start of synset definition
    if (trimmed.startsWith('wnid:oewn-') && trimmed.includes('-n')) {
      if (currentSynset && synsets.length < limit) {
        synsets.push(currentSynset);
      }

      if (synsets.length >= limit) break;

      const synsetId = trimmed.replace(/\s.*$/, '');
      currentSynset = {
        id: synsetId,
        definition: null,
        example: null,
        hypernyms: [],
        hyponyms: [],
        lemmas: []
      };
      inSynset = true;
    }

    // Parse synset properties
    if (inSynset && currentSynset) {
      // Definition
      if (trimmed.includes('wn:definition')) {
        const match = trimmed.match(/rdf:value\s+"([^"]+)"/);
        if (match) currentSynset.definition = match[1];
      }

      // Example
      if (trimmed.includes('wn:example')) {
        const match = trimmed.match(/rdf:value\s+"([^"]+)"/);
        if (match) currentSynset.example = match[1];
      }

      // Hypernym (parent class)
      if (trimmed.includes('wn:hypernym')) {
        const match = trimmed.match(/wnid:(oewn-\d+-[nvar])/g);
        if (match) currentSynset.hypernyms.push(...match.map(m => m.replace('wnid:', '')));
      }

      // Hyponym (child class)
      if (trimmed.includes('wn:hyponym')) {
        const match = trimmed.match(/wnid:(oewn-\d+-[nvar])/g);
        if (match) currentSynset.hyponyms.push(...match.map(m => m.replace('wnid:', '')));
      }

      // End of synset block
      if (trimmed === '.') {
        inSynset = false;
      }
    }
  }

  // Add last synset
  if (currentSynset && synsets.length < limit) {
    synsets.push(currentSynset);
  }

  console.log(`✅ Parsed ${synsets.length} synsets`);
  return synsets;
}

/**
 * Extract lemma (word) from synset ID by looking up in WordNet
 */
function extractLemmaFromSynset(synsetId, content) {
  // Try to find associated lemma entry
  const regex = new RegExp(`<https://en-word.net/lemma/([^#]+)#[^>]+-n>\\s+a ontolex:LexicalEntry[\\s\\S]*?ontolex:sense <[^>]*${synsetId}[^>]*>`);
  const match = content.match(regex);

  if (match) {
    return match[1].replace(/_/g, ' ');
  }

  // Fallback: use synset ID
  return synsetId.replace('oewn-', 'concept-');
}

/**
 * Convert synset ID to class name (PascalCase)
 */
function synsetToClassName(synsetId, lemma) {
  // Use lemma if available, otherwise fallback to ID
  if (lemma && lemma !== synsetId) {
    return lemma
      .split(/[\s_-]+/)
      .map(word => word.charAt(0).toUpperCase() + word.slice(1).toLowerCase())
      .join('');
  }
  return 'Concept_' + synsetId.replace('oewn-', '').replace(/-/g, '_');
}

/**
 * Initialize SQLite database with schema
 */
function initDatabase(dbPath) {
  console.log(`\n🗄️  Initializing database at ${dbPath}...`);

  // Remove existing database
  if (fs.existsSync(dbPath)) {
    fs.unlinkSync(dbPath);
    console.log('   Removed existing database');
  }

  const db = new Database(dbPath);

  // Create schema
  db.exec(`
    -- Main triples table (append-only, immutable)
    CREATE TABLE triples (
      -- RDF Triple (core)
      subject TEXT NOT NULL,
      predicate TEXT NOT NULL,

      -- Object (one of three forms)
      object TEXT,
      object_value TEXT,
      object_datatype TEXT,
      object_language TEXT,
      object_type TEXT NOT NULL CHECK(object_type IN ('iri', 'literal', 'blank')),

      -- Performance optimization: typed columns (NULL if not applicable)
      object_number REAL,
      object_integer INTEGER,
      object_datetime INTEGER,
      object_boolean INTEGER,

      -- SuperNOVA extensions: transaction metadata
      tx INTEGER NOT NULL,
      origin TEXT NOT NULL,
      retracted INTEGER NOT NULL DEFAULT 0,
      created_at INTEGER NOT NULL,

      -- Consistency constraints
      CHECK (
        (object_type = 'iri' AND object IS NOT NULL AND object_value IS NULL) OR
        (object_type = 'literal' AND object_value IS NOT NULL AND object_datatype IS NOT NULL AND object IS NULL) OR
        (object_type = 'blank' AND object IS NOT NULL AND object_value IS NULL)
      )
    );

    -- Indices for efficient queries
    CREATE INDEX idx_spo ON triples(subject, predicate, object, object_value, tx, origin);
    CREATE INDEX idx_pos ON triples(predicate, object, object_value, subject, tx, origin);
    CREATE INDEX idx_osp ON triples(object, subject, predicate, tx, origin) WHERE object_type = 'iri';
    CREATE INDEX idx_ops ON triples(object, predicate, subject, tx, origin) WHERE object_type = 'iri';

    -- Transaction log
    CREATE TABLE transactions (
      tx INTEGER PRIMARY KEY AUTOINCREMENT,
      origin TEXT NOT NULL,
      created_at INTEGER NOT NULL
    );
  `);

  console.log('   ✅ Schema created');
  return db;
}

/**
 * Insert triple into database
 */
function insertTriple(db, subject, predicate, objectOrValue, isLiteral = false, datatype = 'xsd:string') {
  const stmt = db.prepare(`
    INSERT INTO triples (
      subject, predicate, object, object_value, object_datatype, object_language,
      object_type, object_number, object_integer, object_datetime, object_boolean,
      tx, origin, retracted, created_at
    ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
  `);

  if (isLiteral) {
    stmt.run(
      subject, predicate, null, objectOrValue, datatype, null,
      'literal', null, null, null, null,
      txCounter, ORIGIN, 0, timestamp
    );
  } else {
    stmt.run(
      subject, predicate, objectOrValue, null, null, null,
      'iri', null, null, null, null,
      txCounter, ORIGIN, 0, timestamp
    );
  }
}

/**
 * Convert synsets to OWL classes and import to database
 */
function convertAndImport(synsets, db, wordnetContent) {
  console.log(`\n🔄 Converting ${synsets.length} synsets to OWL classes...`);

  const transaction = db.transaction((synsets) => {
    let classCount = 0;

    for (const synset of synsets) {
      const lemma = extractLemmaFromSynset(synset.id, wordnetContent);
      const className = synsetToClassName(synset.id, lemma);
      const classIRI = `sn:${className}`;

      console.log(`\n   📦 ${classIRI}`);
      console.log(`      Definition: ${synset.definition || 'N/A'}`);

      // Create new transaction for this class
      txCounter++;

      // Class declaration
      insertTriple(db, classIRI, 'rdf:type', 'owl:Class');
      console.log(`      ✓ rdf:type owl:Class`);

      // Label
      if (lemma) {
        insertTriple(db, classIRI, 'rdfs:label', lemma, true, 'xsd:string');
        console.log(`      ✓ rdfs:label "${lemma}"`);
      }

      // Definition as comment
      if (synset.definition) {
        insertTriple(db, classIRI, 'rdfs:comment', synset.definition, true, 'xsd:string');
        console.log(`      ✓ rdfs:comment`);
      }

      // Example
      if (synset.example) {
        insertTriple(db, classIRI, 'skos:example', synset.example, true, 'xsd:string');
        console.log(`      ✓ skos:example`);
      }

      // Hypernyms (subClassOf)
      for (const hypernym of synset.hypernyms) {
        const parentClass = `sn:Concept_${hypernym.replace('oewn-', '').replace(/-/g, '_')}`;
        insertTriple(db, classIRI, 'rdfs:subClassOf', parentClass);
        console.log(`      ✓ rdfs:subClassOf ${parentClass}`);
      }

      // Link back to WordNet synset
      insertTriple(db, classIRI, 'sn:derivedFrom', `wnid:${synset.id}`);
      console.log(`      ✓ sn:derivedFrom wnid:${synset.id}`);

      classCount++;
    }

    console.log(`\n✅ Converted ${classCount} classes`);
  });

  transaction(synsets);
}

/**
 * Main execution
 */
function main() {
  console.log('🚀 SuperNOVA Ontology Builder\n');
  console.log('================================\n');

  // Parse WordNet (limit to 10 synsets for testing)
  const synsets = parseWordNetSynsets(WORDNET_PATH, 10);

  if (synsets.length === 0) {
    console.error('❌ No synsets found. Check WordNet file format.');
    process.exit(1);
  }

  // Read full content for lemma extraction
  const wordnetContent = fs.readFileSync(WORDNET_PATH, 'utf-8');

  // Initialize database
  const db = initDatabase(DB_PATH);

  // Convert and import
  convertAndImport(synsets, db, wordnetContent);

  // Stats
  const stats = db.prepare('SELECT COUNT(*) as count FROM triples').get();
  const txStats = db.prepare('SELECT MAX(tx) as max_tx FROM triples').get();

  console.log('\n================================');
  console.log('📊 Database Statistics:\n');
  console.log(`   Triples inserted: ${stats.count}`);
  console.log(`   Transactions used: ${txStats.max_tx}`);
  console.log(`   Database size: ${(fs.statSync(DB_PATH).size / 1024).toFixed(2)} KB`);
  console.log(`\n✅ Database ready: ${DB_PATH}`);
  console.log('\nNext steps:');
  console.log('  - Review database content');
  console.log('  - Test with queries');
  console.log('  - Commit supernova.db to git');

  db.close();
}

main();
