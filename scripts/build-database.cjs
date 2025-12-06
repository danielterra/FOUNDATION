#!/usr/bin/env node

/**
 * FOUNDATION Database Builder
 *
 * Builds the complete FOUNDATION.db with:
 * 1. Schema (tables, indices, views)
 * 2. RDF/RDFS/OWL core ontology
 * 3. FOUNDATION base ontology (abstract concepts)
 *
 * This script is run during development setup and before releases.
 * The resulting database is versioned in git.
 */

const Database = require('better-sqlite3');
const fs = require('fs');
const path = require('path');
const { createReadStream } = require('fs');
const { createInterface } = require('readline');

const PROJECT_ROOT = path.join(__dirname, '..');
const DB_PATH = path.join(PROJECT_ROOT, 'FOUNDATION.db');
const SCHEMA_PATH = path.join(PROJECT_ROOT, 'db', 'schema.sql');
const CORE_ONTOLOGY_DIR = path.join(PROJECT_ROOT, 'core-ontology');
const RDF_CORE_PATH = path.join(CORE_ONTOLOGY_DIR, 'rdf-rdfs-owl-core.ttl');

// Extract class definitions and their dependencies from a .ttl file
function extractDependencies(filePath) {
  const content = fs.readFileSync(filePath, 'utf-8');
  const classes = [];
  const dependencies = new Set();

  // Match class definitions: foundation:ClassName a owl:Class
  const classRegex = /foundation:(\w+)\s+a\s+owl:Class/g;
  let match;
  while ((match = classRegex.exec(content)) !== null) {
    classes.push(match[1]);
  }

  // Match subclass declarations: rdfs:subClassOf foundation:ParentClass or owl:Thing
  const subClassRegex = /rdfs:subClassOf\s+(?:foundation:(\w+)|owl:Thing)/g;
  while ((match = subClassRegex.exec(content)) !== null) {
    if (match[1]) { // Only foundation: classes (skip owl:Thing)
      dependencies.add(match[1]);
    }
  }

  return { classes, dependencies: Array.from(dependencies) };
}

// Topological sort to resolve dependencies
function topologicalSort(files) {
  const graph = new Map(); // filename -> { classes: [], deps: [] }
  const classToFile = new Map(); // className -> filename

  // Build dependency graph
  for (const file of files) {
    const filename = path.basename(file);
    const { classes, dependencies } = extractDependencies(file);
    graph.set(filename, { classes, dependencies });

    // Map classes to their defining file
    for (const cls of classes) {
      classToFile.set(cls, filename);
    }
  }

  // Convert class dependencies to file dependencies
  const fileDeps = new Map();
  for (const [filename, { dependencies }] of graph) {
    const depFiles = new Set();
    for (const dep of dependencies) {
      const depFile = classToFile.get(dep);
      if (depFile && depFile !== filename) {
        depFiles.add(depFile);
      }
    }
    fileDeps.set(filename, Array.from(depFiles));
  }

  // Topological sort (Kahn's algorithm)
  const sorted = [];
  const inDegree = new Map();

  // Calculate in-degrees (how many files depend on each file)
  for (const filename of fileDeps.keys()) {
    inDegree.set(filename, fileDeps.get(filename).length);
  }

  // Queue files with no dependencies
  const queue = Array.from(inDegree.entries())
    .filter(([_, degree]) => degree === 0)
    .map(([file, _]) => file);

  while (queue.length > 0) {
    const file = queue.shift();
    sorted.push(file);

    // Find files that depend on this file and reduce their in-degree
    for (const [dependent, deps] of fileDeps.entries()) {
      if (deps.includes(file)) {
        const newDegree = inDegree.get(dependent) - 1;
        inDegree.set(dependent, newDegree);
        if (newDegree === 0 && !sorted.includes(dependent)) {
          queue.push(dependent);
        }
      }
    }
  }

  // Check for cycles
  if (sorted.length !== fileDeps.size) {
    console.error('⚠️  Warning: Circular dependencies detected in ontology files');
    console.error('Missing files:', Array.from(fileDeps.keys()).filter(f => !sorted.includes(f)));
    // Add remaining files in alphabetical order
    const remaining = Array.from(fileDeps.keys()).filter(f => !sorted.includes(f)).sort();
    sorted.push(...remaining);
  }

  return sorted.map(file => path.join(CORE_ONTOLOGY_DIR, file));
}

// Dynamically import all .ttl files from core-ontology (except rdf-rdfs-owl-core.ttl)
// Automatically resolves dependencies by analyzing rdfs:subClassOf declarations
function getFoundationOntologyFiles() {
  const allFiles = fs.readdirSync(CORE_ONTOLOGY_DIR)
    .filter(file => file.endsWith('.ttl') && file !== 'rdf-rdfs-owl-core.ttl')
    .map(file => path.join(CORE_ONTOLOGY_DIR, file));

  return topologicalSort(allFiles);
}

// Namespace map for compact IRI representation
const NAMESPACES = {
  'http://www.w3.org/1999/02/22-rdf-syntax-ns#': 'rdf:',
  'http://www.w3.org/2000/01/rdf-schema#': 'rdfs:',
  'http://www.w3.org/2002/07/owl#': 'owl:',
  'http://www.w3.org/2001/XMLSchema#': 'xsd:',
  'http://FOUNDATION.local/ontology/': 'foundation:'
};

/**
 * Compress a full IRI to prefixed form
 * Example: "http://www.w3.org/2000/01/rdf-schema#label" -> "rdfs:label"
 */
function compressIRI(iri) {
  for (const [namespace, prefix] of Object.entries(NAMESPACES)) {
    if (iri.startsWith(namespace)) {
      return iri.replace(namespace, prefix);
    }
  }
  return iri; // Return as-is if no namespace match
}

/**
 * Expand a prefixed IRI to full form
 * Example: "rdfs:label" -> "http://www.w3.org/2000/01/rdf-schema#label"
 */
function expandIRI(prefixed) {
  for (const [namespace, prefix] of Object.entries(NAMESPACES)) {
    if (prefixed.startsWith(prefix)) {
      return prefixed.replace(prefix, namespace);
    }
  }
  return prefixed; // Return as-is if no prefix match
}

async function main() {
  console.log('🚀 FOUNDATION Database Builder\n');
  console.log('================================\n');

  // Step 1: Create database and schema
  console.log('📋 Creating schema...');
  const db = new Database(DB_PATH);
  const schema = fs.readFileSync(SCHEMA_PATH, 'utf-8');
  db.exec(schema);
  console.log('✅ Schema created\n');

  // Step 2: Import RDF/RDFS/OWL core ontology
  console.log('📚 Importing RDF/RDFS/OWL core ontology...');
  let tx = 1;
  const coreTriples = await importTurtleFile(RDF_CORE_PATH, db, tx, 1); // origin_id = 1 (rdf:core)
  console.log(`✅ Imported ${coreTriples} triples from RDF/RDFS/OWL\n`);
  tx += coreTriples;

  // Step 3: Import FOUNDATION ontology (dynamically discover all .ttl files)
  console.log('🏛️  Importing FOUNDATION ontology...');
  const foundationFiles = getFoundationOntologyFiles();
  let totalBaseTriples = 0;

  // Create origins for each file and get their IDs
  const getOrCreateOrigin = db.prepare(`
    INSERT INTO origins (name, description) VALUES (?, ?)
    ON CONFLICT(name) DO UPDATE SET name=name
    RETURNING id
  `);

  for (const ontologyFile of foundationFiles) {
    const filename = path.basename(ontologyFile);
    const originName = `foundation:ontology:${filename}`;
    const originDesc = `FOUNDATION core ontology file: ${filename}`;

    // Get or create origin for this file
    const { id: originId } = getOrCreateOrigin.get(originName, originDesc);

    console.log(`   📄 ${filename}...`);
    const triples = await importTurtleFile(ontologyFile, db, tx, originId);
    totalBaseTriples += triples;
    tx += triples;
  }
  console.log(`✅ Imported ${totalBaseTriples} triples from FOUNDATION ontology (${foundationFiles.length} files)\n`);

  // Step 4: Set metadata
  console.log('⚙️  Setting metadata...');
  db.prepare("UPDATE metadata SET value = 'true', updated_at = ? WHERE key = 'ontology_imported'")
    .run(Date.now());
  console.log('✅ Metadata updated\n');

  // Step 5: Print statistics
  const stats = db.prepare(`
    SELECT
      (SELECT COUNT(*) FROM triples) as total_triples,
      (SELECT COUNT(*) FROM triples WHERE retracted = 0) as active_triples,
      (SELECT MAX(tx) FROM triples) as max_tx,
      (SELECT COUNT(DISTINCT subject) FROM triples WHERE retracted = 0) as entities
  `).get();

  console.log('================================');
  console.log('📊 Database Statistics:\n');
  console.log(`   Total triples: ${stats.total_triples}`);
  console.log(`   Active triples: ${stats.active_triples}`);
  console.log(`   Transactions: ${stats.max_tx}`);
  console.log(`   Entities: ${stats.entities}`);

  const sizeBytes = fs.statSync(DB_PATH).size;
  const sizeKB = (sizeBytes / 1024).toFixed(2);
  console.log(`   Size: ${sizeKB} KB\n`);

  console.log(`✅ Database ready: ${DB_PATH}`);

  db.close();
}

main().catch(err => {
  console.error('❌ Database build failed:', err);
  process.exit(1);
});

// ============================================================================
// Helper Functions
// ============================================================================

/**
 * Import Turtle file using N3 parser
 */
async function importTurtleFile(filePath, db, startTx, originId) {
  const { default: N3Parser } = await import('@rdfjs/parser-n3');
  const { default: streamifyString } = await import('streamify-string');
  const rdfParser = new N3Parser();
  const content = fs.readFileSync(filePath, 'utf-8');
  const stream = rdfParser.import(streamifyString(content));

  let tx = startTx;
  let count = 0;
  const timestamp = Date.now();

  // Begin transaction
  const insertTriple = db.prepare(`
    INSERT INTO triples (
      subject, predicate, object, object_value, object_type, object_datatype,
      object_language, object_number, object_integer, object_datetime, object_boolean,
      tx, origin_id, retracted, created_at
    ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 0, ?)
  `);

  const insert = db.transaction((triples) => {
    for (const triple of triples) {
      insertTriple.run(
        triple.subject,
        triple.predicate,
        triple.object,
        triple.object_value,
        triple.object_type,
        triple.object_datatype,
        triple.object_language,
        triple.object_number,
        triple.object_integer,
        triple.object_datetime,
        triple.object_boolean,
        triple.tx,
        triple.origin_id,
        triple.created_at
      );
    }
  });

  // Parse and collect triples
  const triples = [];

  // Consume the stream
  for await (const quad of stream) {
    const subject = compressIRI(quad.subject.value);
    const predicate = compressIRI(quad.predicate.value);

    let object = null;
    let object_value = null;
    let object_type = null;
    let object_datatype = null;
    let object_language = null;
    let object_number = null;
    let object_integer = null;
    let object_datetime = null;
    let object_boolean = null;

    if (quad.object.termType === 'Literal') {
      object_type = 'literal';
      object_value = quad.object.value;
      object_datatype = compressIRI(quad.object.datatype?.value || 'http://www.w3.org/2001/XMLSchema#string');
      object_language = quad.object.language || null;

      // Parse typed values
      if (object_datatype.includes('decimal') || object_datatype.includes('double') || object_datatype.includes('float')) {
        object_number = parseFloat(object_value);
      } else if (object_datatype.includes('integer') || object_datatype.includes('int') || object_datatype.includes('long')) {
        object_integer = parseInt(object_value, 10);
      } else if (object_datatype.includes('boolean')) {
        object_boolean = object_value === 'true' ? 1 : 0;
      }
    } else if (quad.object.termType === 'BlankNode') {
      object_type = 'blank';
      object = quad.object.value;
    } else {
      object_type = 'iri';
      object = compressIRI(quad.object.value);
    }

    triples.push({
      subject,
      predicate,
      object,
      object_value,
      object_type,
      object_datatype,
      object_language,
      object_number,
      object_integer,
      object_datetime,
      object_boolean,
      tx: tx++,
      origin_id: originId,
      created_at: timestamp
    });
    count++;
  }

  insert(triples);
  return count;
}
