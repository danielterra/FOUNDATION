#!/usr/bin/env node

/**
 * FOUNDATION Ontology Updater
 *
 * Incrementally updates the core ontology in FOUNDATION.db by:
 * 1. Tracking each .ttl file as a DigitalThing
 * 2. Comparing lastModified vs lastImported timestamps
 * 3. Retracting old triples and importing new ones
 * 4. Preserving complete audit trail (immutable)
 *
 * Usage: npm run update:ontology
 */

const Database = require('better-sqlite3');
const fs = require('fs');
const path = require('path');
const crypto = require('crypto');
const { createReadStream } = require('fs');
const { createInterface } = require('readline');

const PROJECT_ROOT = path.join(__dirname, '..');
const DB_PATH = path.join(PROJECT_ROOT, 'FOUNDATION.db');
const CORE_ONTOLOGY_DIR = path.join(PROJECT_ROOT, 'core-ontology');

// Namespace mappings
const NAMESPACES = {
  'http://www.w3.org/1999/02/22-rdf-syntax-ns#': 'rdf:',
  'http://www.w3.org/2000/01/rdf-schema#': 'rdfs:',
  'http://www.w3.org/2002/07/owl#': 'owl:',
  'http://www.w3.org/2001/XMLSchema#': 'xsd:',
  'http://FOUNDATION.local/ontology/': 'foundation:'
};

/**
 * Calculate SHA-256 checksum of file
 */
function calculateChecksum(filePath) {
  const content = fs.readFileSync(filePath, 'utf-8');
  return crypto.createHash('sha256').update(content).digest('hex');
}

/**
 * Get file modification time as ISO string
 */
function getFileModTime(filePath) {
  const stats = fs.statSync(filePath);
  return stats.mtime.toISOString();
}

/**
 * Compress IRI to prefixed form
 */
function compressIRI(iri) {
  for (const [namespace, prefix] of Object.entries(NAMESPACES)) {
    if (iri.startsWith(namespace)) {
      return iri.replace(namespace, prefix);
    }
  }
  return iri;
}

/**
 * Get tracked file info from database
 * Returns { lastImported, checksum, tripleCount } or null if not tracked
 */
function getTrackedFileInfo(db, filePath) {
  const fileIRI = `http://FOUNDATION.local/ontology/file:${path.basename(filePath)}`;

  const query = `
    SELECT
      MAX(CASE WHEN predicate = 'http://FOUNDATION.local/ontology/lastImported' THEN object_value END) as lastImported,
      MAX(CASE WHEN predicate = 'http://FOUNDATION.local/ontology/checksum' THEN object_value END) as checksum,
      MAX(CASE WHEN predicate = 'http://FOUNDATION.local/ontology/tripleCount' THEN object_integer END) as tripleCount
    FROM triples
    WHERE subject = ? AND retracted = 0
  `;

  return db.prepare(query).get(fileIRI);
}

/**
 * Parse Turtle file and import triples
 */
async function importTurtleFile(filePath, db, startTx, originId) {
  const fileStream = createReadStream(filePath);
  const rl = createInterface({ input: fileStream, crlfDelay: Infinity });

  let tripleCount = 0;
  let currentTx = startTx;
  const now = Date.now();

  const insertStmt = db.prepare(`
    INSERT INTO triples (
      subject, predicate, object, object_value, object_datatype, object_language,
      object_type, tx, origin_id, retracted, created_at
    ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, 0, ?)
  `);

  let currentSubject = null;
  let currentPredicate = null;

  for await (const line of rl) {
    const trimmed = line.trim();

    // Skip comments and empty lines
    if (!trimmed || trimmed.startsWith('#') || trimmed.startsWith('@')) continue;

    // Simple Turtle parser (handles basic subject-predicate-object patterns)
    const tripleMatch = trimmed.match(/^([^\s]+)\s+([^\s]+)\s+(.+?)\s*[;.]?\s*$/);

    if (tripleMatch) {
      let [_, subj, pred, obj] = tripleMatch;

      // Handle abbreviated syntax
      if (subj !== currentSubject && !subj.startsWith('_:')) {
        currentSubject = subj.includes(':') ? subj : currentSubject;
      }

      if (pred === 'a') pred = 'rdf:type';
      currentPredicate = pred;

      // Clean object
      obj = obj.replace(/[;.]$/, '').trim();

      // Expand prefixes (simple expansion)
      const expandIRI = (iri) => {
        for (const [namespace, prefix] of Object.entries(NAMESPACES)) {
          if (iri.startsWith(prefix)) {
            return iri.replace(prefix, namespace);
          }
        }
        return iri;
      };

      const subject = expandIRI(currentSubject);
      const predicate = expandIRI(pred);

      let objectType, object, objectValue, objectDatatype, objectLanguage;

      // Determine object type
      if (obj.startsWith('"')) {
        // Literal
        objectType = 'literal';

        // Extract literal parts
        const literalMatch = obj.match(/^"([^"]*)"(?:\^\^(.+)|@(.+))?$/);
        if (literalMatch) {
          objectValue = literalMatch[1];
          objectDatatype = literalMatch[2] ? expandIRI(literalMatch[2]) : 'http://www.w3.org/2001/XMLSchema#string';
          objectLanguage = literalMatch[3] || null;
          object = null;
        }
      } else {
        // IRI or blank node
        objectType = obj.startsWith('_:') ? 'blank' : 'iri';
        object = expandIRI(obj);
        objectValue = null;
        objectDatatype = null;
        objectLanguage = null;
      }

      // Insert triple
      insertStmt.run(
        subject, predicate, object, objectValue, objectDatatype, objectLanguage,
        objectType, currentTx, originId, now
      );

      tripleCount++;
      currentTx++;
    }
  }

  return tripleCount;
}

/**
 * Get or create origin for a file
 */
function getOrCreateOrigin(db, filename) {
  const originName = `foundation:ontology:${filename}`;
  const originDesc = `FOUNDATION core ontology file: ${filename}`;

  const getOrCreateStmt = db.prepare(`
    INSERT INTO origins (name, description) VALUES (?, ?)
    ON CONFLICT(name) DO UPDATE SET name=name
    RETURNING id
  `);

  const result = getOrCreateStmt.get(originName, originDesc);
  return result.id;
}

/**
 * Retract all triples from a specific file
 */
function retractFileTriples(db, filePath) {
  const filename = path.basename(filePath);
  const originName = `foundation:ontology:${filename}`;

  // Get origin_id for this file
  const origin = db.prepare('SELECT id FROM origins WHERE name = ?').get(originName);

  if (!origin) {
    console.log(`   ⚠️  No previous import found for ${filename}`);
    return 0;
  }

  // Retract all triples with this origin_id
  const result = db.prepare(`
    UPDATE triples
    SET retracted = 1
    WHERE origin_id = ? AND retracted = 0
  `).run(origin.id);

  return result.changes;
}

/**
 * Update file tracking metadata
 */
function updateFileMetadata(db, filePath, modTime, checksum, tripleCount, tx) {
  const fileIRI = `http://FOUNDATION.local/ontology/file:${path.basename(filePath)}`;
  const now = Date.now();
  const originId = 1; // rdf:core

  // Retract old metadata
  db.prepare(`
    UPDATE triples
    SET retracted = 1
    WHERE subject = ? AND retracted = 0
  `).run(fileIRI);

  // Insert new metadata
  const insertStmt = db.prepare(`
    INSERT INTO triples (
      subject, predicate, object, object_value, object_datatype, object_language,
      object_type, object_integer, tx, origin_id, retracted, created_at
    ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 0, ?)
  `);

  const baseTx = tx;

  // rdf:type foundation:DigitalThing
  insertStmt.run(
    fileIRI,
    'http://www.w3.org/1999/02/22-rdf-syntax-ns#type',
    'http://FOUNDATION.local/ontology/DigitalThing',
    null, null, null, 'iri', null, baseTx, originId, now
  );

  // foundation:filePath
  insertStmt.run(
    fileIRI,
    'http://FOUNDATION.local/ontology/filePath',
    null, `core-ontology/${path.basename(filePath)}`,
    'http://www.w3.org/2001/XMLSchema#string',
    null, 'literal', null, baseTx + 1, originId, now
  );

  // foundation:lastModified
  insertStmt.run(
    fileIRI,
    'http://FOUNDATION.local/ontology/lastModified',
    null, modTime,
    'http://www.w3.org/2001/XMLSchema#dateTime',
    null, 'literal', null, baseTx + 2, originId, now
  );

  // foundation:lastImported
  insertStmt.run(
    fileIRI,
    'http://FOUNDATION.local/ontology/lastImported',
    null, new Date().toISOString(),
    'http://www.w3.org/2001/XMLSchema#dateTime',
    null, 'literal', null, baseTx + 3, originId, now
  );

  // foundation:checksum
  insertStmt.run(
    fileIRI,
    'http://FOUNDATION.local/ontology/checksum',
    null, checksum,
    'http://www.w3.org/2001/XMLSchema#string',
    null, 'literal', null, baseTx + 4, originId, now
  );

  // foundation:tripleCount
  insertStmt.run(
    fileIRI,
    'http://FOUNDATION.local/ontology/tripleCount',
    null, tripleCount.toString(),
    'http://www.w3.org/2001/XMLSchema#integer',
    null, 'literal', tripleCount,
    baseTx + 5, originId, now
  );

  return 6; // Number of metadata triples added
}

async function main() {
  console.log('🔄 FOUNDATION Ontology Updater\n');
  console.log('================================\n');

  if (!fs.existsSync(DB_PATH)) {
    console.error('❌ Database not found! Run `npm run build:db` first.');
    process.exit(1);
  }

  const db = new Database(DB_PATH);

  // Get current max transaction ID
  const { max_tx } = db.prepare('SELECT MAX(tx) as max_tx FROM triples').get();
  let currentTx = (max_tx || 0) + 1;

  // Get all .ttl files except rdf-rdfs-owl-core.ttl
  const ontologyFiles = fs.readdirSync(CORE_ONTOLOGY_DIR)
    .filter(f => f.endsWith('.ttl') && f !== 'rdf-rdfs-owl-core.ttl')
    .map(f => path.join(CORE_ONTOLOGY_DIR, f));

  console.log(`📂 Found ${ontologyFiles.length} ontology files\n`);

  let updatedCount = 0;
  let skippedCount = 0;

  for (const filePath of ontologyFiles) {
    const filename = path.basename(filePath);
    const modTime = getFileModTime(filePath);
    const checksum = calculateChecksum(filePath);
    const tracked = getTrackedFileInfo(db, filePath);

    console.log(`📄 ${filename}`);

    // Check if file needs update
    const needsUpdate = !tracked ||
                        tracked.checksum !== checksum ||
                        !tracked.lastImported;

    if (!needsUpdate) {
      console.log(`   ✅ Up to date (last imported: ${tracked.lastImported})\n`);
      skippedCount++;
      continue;
    }

    // File needs update
    if (tracked) {
      console.log(`   🔄 Checksum changed, updating...`);
      const retractedCount = retractFileTriples(db, filePath);
      console.log(`   📤 Retracted ${retractedCount} triples from previous import`);
    } else {
      console.log(`   ➕ New file, importing...`);
    }

    // Get or create origin for this file
    const originId = getOrCreateOrigin(db, filename);

    // Import new triples
    const tripleCount = await importTurtleFile(filePath, db, currentTx, originId);
    console.log(`   📥 Imported ${tripleCount} triples`);
    currentTx += tripleCount;

    // Update metadata
    const metaTriples = updateFileMetadata(db, filePath, modTime, checksum, tripleCount, currentTx);
    currentTx += metaTriples;

    console.log(`   ✅ Updated metadata\n`);
    updatedCount++;
  }

  // Print summary
  console.log('================================');
  console.log('📊 Update Summary:\n');
  console.log(`   Updated: ${updatedCount} files`);
  console.log(`   Skipped: ${skippedCount} files (already up to date)`);

  const stats = db.prepare(`
    SELECT
      (SELECT COUNT(*) FROM triples WHERE retracted = 0) as active_triples,
      (SELECT MAX(tx) FROM triples) as max_tx
  `).get();

  console.log(`\n   Active triples: ${stats.active_triples}`);
  console.log(`   Latest transaction: ${stats.max_tx}`);

  console.log('\n✅ Ontology update complete!');

  db.close();
}

main().catch(err => {
  console.error('❌ Error:', err);
  process.exit(1);
});
