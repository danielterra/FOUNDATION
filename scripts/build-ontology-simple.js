#!/usr/bin/env node

/**
 * Build SuperNOVA Base Ontology from WordNet (Simplified version for testing)
 */

import fs from 'fs';
import path from 'path';
import { fileURLToPath } from 'url';
import Database from 'better-sqlite3';
import { createReadStream } from 'fs';
import { createInterface } from 'readline';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const PROJECT_ROOT = path.resolve(__dirname, '..');
const WORDNET_PATH = path.join(PROJECT_ROOT, 'core-ontology/english-wordnet-2024.ttl');
const DB_PATH = path.join(PROJECT_ROOT, 'supernova.db'); // Single database at project root

let txCounter = 1;
const ORIGIN = 'core';
const timestamp = Date.now();

/**
 * Parse WordNet line by line (memory efficient)
 */
async function parseWordNetSynsets(filePath, limit = 10) {
  console.log(`📖 Parsing first ${limit} noun synsets from WordNet...`);

  const synsets = [];
  let currentSynset = null;
  let inSynset = false;

  const fileStream = createReadStream(filePath);
  const rl = createInterface({ input: fileStream, crlfDelay: Infinity });

  for await (const line of rl) {
    const trimmed = line.trim();

    // Start of noun synset definition
    if (trimmed.startsWith('wnid:oewn-') && trimmed.includes('-n')) {
      if (currentSynset) {
        synsets.push(currentSynset);
        if (synsets.length >= limit) break;
      }

      const synsetId = trimmed.split(/\s/)[0];
      currentSynset = {
        id: synsetId,
        definition: null,
        example: null,
        hypernyms: []
      };
      inSynset = true;
      continue;
    }

    if (inSynset && currentSynset) {
      // Definition
      if (trimmed.includes('wn:definition')) {
        const match = trimmed.match(/rdf:value\s+"([^"]+)"/);
        if (match) currentSynset.definition = match[1].substring(0, 200); // Limit length
      }

      // Example
      if (trimmed.includes('wn:example')) {
        const match = trimmed.match(/rdf:value\s+"([^"]+)"/);
        if (match) currentSynset.example = match[1].substring(0, 200);
      }

      // Hypernym
      if (trimmed.includes('wn:hypernym ')) {
        const match = trimmed.match(/wnid:(oewn-\d+-n)/);
        if (match) currentSynset.hypernyms.push(match[1]);
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

  rl.close();
  fileStream.close();

  console.log(`✅ Found ${synsets.length} synsets\n`);
  return synsets;
}

/**
 * Initialize database
 */
function initDatabase(dbPath) {
  console.log(`🗄️  Initializing database...`);

  if (fs.existsSync(dbPath)) {
    fs.unlinkSync(dbPath);
  }

  const db = new Database(dbPath);

  db.exec(`
    CREATE TABLE triples (
      subject TEXT NOT NULL,
      predicate TEXT NOT NULL,
      object TEXT,
      object_value TEXT,
      object_datatype TEXT,
      object_language TEXT,
      object_type TEXT NOT NULL CHECK(object_type IN ('iri', 'literal', 'blank')),
      object_number REAL,
      object_integer INTEGER,
      object_datetime INTEGER,
      object_boolean INTEGER,
      tx INTEGER NOT NULL,
      origin TEXT NOT NULL,
      retracted INTEGER NOT NULL DEFAULT 0,
      created_at INTEGER NOT NULL
    );

    CREATE INDEX idx_spo ON triples(subject, predicate, tx);
    CREATE INDEX idx_pos ON triples(predicate, object, tx);

    CREATE TABLE transactions (
      tx INTEGER PRIMARY KEY AUTOINCREMENT,
      origin TEXT NOT NULL,
      created_at INTEGER NOT NULL
    );
  `);

  console.log(`✅ Schema created\n`);
  return db;
}

/**
 * Insert triple
 */
function insertTriple(stmt, subject, predicate, objectOrValue, isLiteral = false) {
  if (isLiteral) {
    stmt.run(
      subject, predicate, null, objectOrValue, 'xsd:string', null,
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
 * Convert synsets to classes
 */
function convertAndImport(synsets, db) {
  console.log(`🔄 Converting synsets to OWL classes...\n`);

  const stmt = db.prepare(`
    INSERT INTO triples (
      subject, predicate, object, object_value, object_datatype, object_language,
      object_type, object_number, object_integer, object_datetime, object_boolean,
      tx, origin, retracted, created_at
    ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
  `);

  db.transaction(() => {
    for (const synset of synsets) {
      const classId = synset.id.replace('wnid:', '');
      const className = `Concept_${classId.replace(/[^a-zA-Z0-9]/g, '_')}`;
      const classIRI = `sn:${className}`;

      console.log(`   📦 ${className}`);
      if (synset.definition) {
        console.log(`      "${synset.definition.substring(0, 60)}..."`);
      }

      txCounter++;

      // Class declaration
      insertTriple(stmt, classIRI, 'rdf:type', 'owl:Class');

      // Definition
      if (synset.definition) {
        insertTriple(stmt, classIRI, 'rdfs:comment', synset.definition, true);
      }

      // Example
      if (synset.example) {
        insertTriple(stmt, classIRI, 'skos:example', synset.example, true);
      }

      // Hypernyms (subClassOf)
      for (const hypernym of synset.hypernyms) {
        const parentClass = `sn:Concept_${hypernym.replace(/[^a-zA-Z0-9]/g, '_')}`;
        insertTriple(stmt, classIRI, 'rdfs:subClassOf', parentClass);
      }

      // Link to WordNet
      insertTriple(stmt, classIRI, 'sn:derivedFrom', synset.id);
    }
  })();

  console.log(`\n✅ Conversion complete`);
}

/**
 * Main
 */
async function main() {
  console.log('🚀 SuperNOVA Ontology Builder\n');
  console.log('================================\n');

  if (!fs.existsSync(WORDNET_PATH)) {
    console.error('❌ WordNet file not found. Run: npm run update:wordnet');
    process.exit(1);
  }

  const synsets = await parseWordNetSynsets(WORDNET_PATH, 10);

  if (synsets.length === 0) {
    console.error('❌ No synsets found');
    process.exit(1);
  }

  const db = initDatabase(DB_PATH);
  convertAndImport(synsets, db);

  const stats = db.prepare('SELECT COUNT(*) as count FROM triples').get();
  const size = (fs.statSync(DB_PATH).size / 1024).toFixed(2);

  console.log('\n================================');
  console.log('📊 Statistics:\n');
  console.log(`   Triples: ${stats.count}`);
  console.log(`   Transactions: ${txCounter}`);
  console.log(`   Size: ${size} KB`);
  console.log(`\n✅ Database ready: ${DB_PATH}\n`);

  db.close();
}

main().catch(err => {
  console.error('❌ Error:', err.message);
  process.exit(1);
});
