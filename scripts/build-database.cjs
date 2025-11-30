#!/usr/bin/env node

/**
 * SuperNOVA Database Builder
 *
 * Builds the complete supernova.db with:
 * 1. Schema (tables, indices, views)
 * 2. RDF/RDFS/OWL core ontology
 * 3. WordNet synsets as OWL classes
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
const DB_PATH = path.join(PROJECT_ROOT, 'supernova.db');
const SCHEMA_PATH = path.join(PROJECT_ROOT, 'db', 'schema.sql');
const RDF_CORE_PATH = path.join(PROJECT_ROOT, 'core-ontology', 'rdf-rdfs-owl-core.ttl');
const WORDNET_PATH = path.join(PROJECT_ROOT, 'core-ontology', 'english-wordnet-2024.ttl');

// Import limits per synset type (Infinity = import all)
const SYNSET_LIMITS = {
  'n': Infinity, // Nouns (84,956 total)
  'v': Infinity, // Verbs (13,830 total)
  'a': Infinity, // Adjectives (7,502 total)
  'r': Infinity  // Adverbs (3,622 total)
};

// Map synset types to OWL constructs
const SYNSET_TYPE_MAP = {
  'n': { prefix: 'Concept_', owlType: 'owl:Class', relationPredicate: 'rdfs:subClassOf' },
  'v': { prefix: 'Property_', owlType: 'owl:ObjectProperty', relationPredicate: 'rdfs:subPropertyOf' },
  'a': { prefix: 'Quality_', owlType: 'owl:Class', relationPredicate: 'skos:broader' },
  'r': { prefix: 'Manner_', owlType: 'owl:Class', relationPredicate: 'skos:broader' }
};

// Namespace map for compact IRI representation
const NAMESPACES = {
  'http://www.w3.org/1999/02/22-rdf-syntax-ns#': 'rdf:',
  'http://www.w3.org/2000/01/rdf-schema#': 'rdfs:',
  'http://www.w3.org/2002/07/owl#': 'owl:',
  'http://www.w3.org/2001/XMLSchema#': 'xsd:',
  'http://www.w3.org/2004/02/skos/core#': 'skos:',
  'http://supernova.local/ontology/': 'supernova:'
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
  console.log('🚀 SuperNOVA Database Builder\n');
  console.log('================================\n');

  // Step 1: Create database and schema
  console.log('📋 Creating schema...');
  const db = new Database(DB_PATH);
  const schema = fs.readFileSync(SCHEMA_PATH, 'utf-8');
  db.exec(schema);
  console.log('✅ Schema created\n');

  // Step 2: Import RDF/RDFS/OWL core ontology
  console.log('📚 Importing RDF/RDFS/OWL core ontology...');
  const coreTriples = await importTurtleFile(RDF_CORE_PATH, db, 1, 1); // origin_id = 1 (rdf:core)
  console.log(`✅ Imported ${coreTriples} triples from RDF/RDFS/OWL\n`);

  // Step 3: Import WordNet synsets (all types)
  console.log(`📖 Importing WordNet synsets (n:${SYNSET_LIMITS.n}, v:${SYNSET_LIMITS.v}, a:${SYNSET_LIMITS.a}, r:${SYNSET_LIMITS.r})...`);
  const wordnetTriples = await importWordNetSynsets(WORDNET_PATH, db, coreTriples + 1, 2); // origin_id = 2 (wordnet:synsets)
  console.log(`✅ Imported ${wordnetTriples} triples from WordNet\n`);

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

/**
 * Import WordNet synsets as OWL constructs (Classes, ObjectProperties, etc.)
 */
async function importWordNetSynsets(filePath, db, startTx, originId) {
  const synsets = await parseWordNetSynsets(filePath, SYNSET_LIMITS);

  let tx = startTx;
  let count = 0;
  const timestamp = Date.now();

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
        null, // language
        null, // number
        null, // integer
        null, // datetime
        null, // boolean
        triple.tx,
        triple.origin_id,
        triple.created_at
      );
    }
  });

  const triples = [];

  for (const synset of synsets) {
    // Get type mapping for this synset
    const typeInfo = SYNSET_TYPE_MAP[synset.type];
    const entityName = `${typeInfo.prefix}${synset.id.replace(/[^a-zA-Z0-9]/g, '_')}`;
    const entityIRI = `supernova:${entityName}`;

    // Triple 1: Entity declaration (rdf:type owl:Class or owl:ObjectProperty)
    triples.push({
      subject: entityIRI,
      predicate: 'rdf:type',
      object: typeInfo.owlType,
      object_value: null,
      object_type: 'iri',
      object_datatype: null,
      tx: tx++,
      origin_id: originId,
      created_at: timestamp
    });
    count++;

    // Triple 2: Label (rdfs:label) - use first lemma, or extract from definition
    let label = null;
    if (synset.lemmas && synset.lemmas.length > 0) {
      label = synset.lemmas[0];  // First lemma is the primary label
    } else if (synset.definition) {
      // Extract the main noun from definition if no lemmas available
      let def = synset.definition;

      // Remove domain context prefix: (baseball), (law), (biology), etc.
      def = def.replace(/^\([^)]+\)\s+/, '');

      // Pattern 1: "the NOUN of/that/which..." -> extract NOUN (single word only)
      // Examples: "the act of hitting" -> "act", "the flight of a ball" -> "flight"
      let match = def.match(/^the\s+([\w-]+)(?:\s+of|\s+that|\s+which|\s+in|\s+for)/);
      if (match && match[1] && match[1].length > 2 && !match[1].match(/^(any|all|some|each|every|no)$/i)) {
        label = match[1];
      } else {
        // Pattern 2: "a/an [adj] NOUN that/which..." - capture last word before that/which
        // Examples: "an entity that exists" -> "entity", "a living organism" -> "organism"
        match = def.match(/^(?:a|an)\s+(?:[\w-]+\s+)*([\w-]+)(?:\s+that|\s+which)/);
        if (match && match[1] && match[1].length > 2 && !match[1].match(/^(any|all|some|each|every|no)$/i)) {
          label = match[1];
        } else {
          // Pattern 3: "NOUN that/which..." - just a noun followed by clause
          // Examples: "organisms that live" -> "organisms"
          match = def.match(/^([\w-]+)(?:\s+that|\s+which)/);
          if (match && match[1] && match[1].length > 2) {
            label = match[1];
          } else {
            // Pattern 4: "a/an NOUN" - simple case
            match = def.match(/^(?:a|an)\s+([\w-]+)$/);
            if (match && match[1] && match[1].length > 2) {
              label = match[1];
            } else {
              // Fallback: use first meaningful word (skip articles and determiners)
              const words = def.split(/\s+/).filter(w => !w.match(/^(a|an|the|any|all|some|each|every|no)$/i));
              label = words.slice(0, Math.min(2, words.length)).join(' ');
            }
          }
        }
      }
    }

    if (label) {
      triples.push({
        subject: entityIRI,
        predicate: 'rdfs:label',
        object: null,
        object_value: label,
        object_type: 'literal',
        object_datatype: 'xsd:string',
        tx: tx++,
        origin_id: originId,
        created_at: timestamp
      });
      count++;
    }

    // Triples 2.1-2.N: Alternative labels (skos:altLabel) - other lemmas as synonyms
    if (synset.lemmas && synset.lemmas.length > 1) {
      for (let i = 1; i < synset.lemmas.length; i++) {
        triples.push({
          subject: entityIRI,
          predicate: 'skos:altLabel',
          object: null,
          object_value: synset.lemmas[i],
          object_type: 'literal',
          object_datatype: 'xsd:string',
          tx: tx++,
          origin_id: originId,
          created_at: timestamp
        });
        count++;
      }
    }

    // Triple 3: Definition (rdfs:comment)
    if (synset.definition) {
      triples.push({
        subject: entityIRI,
        predicate: 'rdfs:comment',
        object: null,
        object_value: synset.definition,
        object_type: 'literal',
        object_datatype: 'xsd:string',
        tx: tx++,
        origin_id: originId,
        created_at: timestamp
      });
      count++;
    }

    // Triple 4: Hierarchical relations (rdfs:subClassOf, rdfs:subPropertyOf, skos:broader)
    // Use appropriate predicate based on synset type
    if (synset.hypernyms.length === 0) {
      // For nouns without hypernyms: make them subclasses of entity (oewn-00001740-n)
      // EXCEPT for entity itself which should be subclass of owl:Thing
      // For verbs without hypernyms: don't add parent (properties don't need owl:Thing as parent)
      // For adjectives/adverbs: don't add parent
      if (synset.type === 'n') {
        const parentIRI = synset.id === 'oewn-00001740-n'
          ? 'owl:Thing'  // entity is child of owl:Thing
          : 'supernova:Concept_oewn_00001740_n';  // all other nouns without hypernym are children of entity

        triples.push({
          subject: entityIRI,
          predicate: typeInfo.relationPredicate,
          object: parentIRI,
          object_value: null,
          object_type: 'iri',
          object_datatype: null,
          tx: tx++,
          origin_id: originId,
          created_at: timestamp
        });
        count++;
      }
    } else {
      // Add all hypernyms with type-specific predicate
      for (const hypernym of synset.hypernyms) {
        const parentEntity = `supernova:${typeInfo.prefix}${hypernym.replace(/[^a-zA-Z0-9]/g, '_')}`;
        triples.push({
          subject: entityIRI,
          predicate: typeInfo.relationPredicate,
          object: parentEntity,
          object_value: null,
          object_type: 'iri',
          object_datatype: null,
          tx: tx++,
          origin_id: originId,
          created_at: timestamp
        });
        count++;
      }
    }

    // Additional semantic relations

    // Similar (skos:related)
    for (const similarId of synset.similar || []) {
      const similarEntity = `supernova:${typeInfo.prefix}${similarId.replace(/[^a-zA-Z0-9]/g, '_')}`;
      triples.push({
        subject: entityIRI,
        predicate: 'skos:related',
        object: similarEntity,
        object_value: null,
        object_type: 'iri',
        object_datatype: null,
        tx: tx++,
        origin_id: originId,
        created_at: timestamp
      });
      count++;
    }

    // Also / See also (rdfs:seeAlso)
    for (const alsoId of synset.also || []) {
      const alsoEntity = `supernova:${typeInfo.prefix}${alsoId.replace(/[^a-zA-Z0-9]/g, '_')}`;
      triples.push({
        subject: entityIRI,
        predicate: 'rdfs:seeAlso',
        object: alsoEntity,
        object_value: null,
        object_type: 'iri',
        object_datatype: null,
        tx: tx++,
        origin_id: originId,
        created_at: timestamp
      });
      count++;
    }

    // Antonym (supernova:antonym)
    for (const antonymId of synset.antonym || []) {
      const antonymEntity = `supernova:${typeInfo.prefix}${antonymId.replace(/[^a-zA-Z0-9]/g, '_')}`;
      triples.push({
        subject: entityIRI,
        predicate: 'supernova:antonym',
        object: antonymEntity,
        object_value: null,
        object_type: 'iri',
        object_datatype: null,
        tx: tx++,
        origin_id: originId,
        created_at: timestamp
      });
      count++;
    }

    // Entails (supernova:entails)
    for (const entailsId of synset.entails || []) {
      const entailsEntity = `supernova:${typeInfo.prefix}${entailsId.replace(/[^a-zA-Z0-9]/g, '_')}`;
      triples.push({
        subject: entityIRI,
        predicate: 'supernova:entails',
        object: entailsEntity,
        object_value: null,
        object_type: 'iri',
        object_datatype: null,
        tx: tx++,
        origin_id: originId,
        created_at: timestamp
      });
      count++;
    }

    // Causes (supernova:causes)
    for (const causesId of synset.causes || []) {
      const causesEntity = `supernova:${typeInfo.prefix}${causesId.replace(/[^a-zA-Z0-9]/g, '_')}`;
      triples.push({
        subject: entityIRI,
        predicate: 'supernova:causes',
        object: causesEntity,
        object_value: null,
        object_type: 'iri',
        object_datatype: null,
        tx: tx++,
        origin_id: originId,
        created_at: timestamp
      });
      count++;
    }

    // Meronym part (supernova:partOf)
    for (const partId of synset.mero_part || []) {
      const partEntity = `supernova:${typeInfo.prefix}${partId.replace(/[^a-zA-Z0-9]/g, '_')}`;
      triples.push({
        subject: entityIRI,
        predicate: 'supernova:partOf',
        object: partEntity,
        object_value: null,
        object_type: 'iri',
        object_datatype: null,
        tx: tx++,
        origin_id: originId,
        created_at: timestamp
      });
      count++;
    }

    // Holonym part (supernova:hasPart)
    for (const partId of synset.holo_part || []) {
      const partEntity = `supernova:${typeInfo.prefix}${partId.replace(/[^a-zA-Z0-9]/g, '_')}`;
      triples.push({
        subject: entityIRI,
        predicate: 'supernova:hasPart',
        object: partEntity,
        object_value: null,
        object_type: 'iri',
        object_datatype: null,
        tx: tx++,
        origin_id: originId,
        created_at: timestamp
      });
      count++;
    }

    // Meronym member (supernova:memberOf)
    for (const memberId of synset.mero_member || []) {
      const memberEntity = `supernova:${typeInfo.prefix}${memberId.replace(/[^a-zA-Z0-9]/g, '_')}`;
      triples.push({
        subject: entityIRI,
        predicate: 'supernova:memberOf',
        object: memberEntity,
        object_value: null,
        object_type: 'iri',
        object_datatype: null,
        tx: tx++,
        origin_id: originId,
        created_at: timestamp
      });
      count++;
    }

    // Holonym member (supernova:hasMember)
    for (const memberId of synset.holo_member || []) {
      const memberEntity = `supernova:${typeInfo.prefix}${memberId.replace(/[^a-zA-Z0-9]/g, '_')}`;
      triples.push({
        subject: entityIRI,
        predicate: 'supernova:hasMember',
        object: memberEntity,
        object_value: null,
        object_type: 'iri',
        object_datatype: null,
        tx: tx++,
        origin_id: originId,
        created_at: timestamp
      });
      count++;
    }

    // Meronym substance (supernova:madeOf)
    for (const substanceId of synset.mero_substance || []) {
      const substanceEntity = `supernova:${typeInfo.prefix}${substanceId.replace(/[^a-zA-Z0-9]/g, '_')}`;
      triples.push({
        subject: entityIRI,
        predicate: 'supernova:madeOf',
        object: substanceEntity,
        object_value: null,
        object_type: 'iri',
        object_datatype: null,
        tx: tx++,
        origin_id: originId,
        created_at: timestamp
      });
      count++;
    }

    // Holonym substance (supernova:containsSubstance)
    for (const substanceId of synset.holo_substance || []) {
      const substanceEntity = `supernova:${typeInfo.prefix}${substanceId.replace(/[^a-zA-Z0-9]/g, '_')}`;
      triples.push({
        subject: entityIRI,
        predicate: 'supernova:containsSubstance',
        object: substanceEntity,
        object_value: null,
        object_type: 'iri',
        object_datatype: null,
        tx: tx++,
        origin_id: originId,
        created_at: timestamp
      });
      count++;
    }

    // Domain topic (supernova:domainTopic)
    for (const topicId of synset.domain_topic || []) {
      const topicEntity = `supernova:${typeInfo.prefix}${topicId.replace(/[^a-zA-Z0-9]/g, '_')}`;
      triples.push({
        subject: entityIRI,
        predicate: 'supernova:domainTopic',
        object: topicEntity,
        object_value: null,
        object_type: 'iri',
        object_datatype: null,
        tx: tx++,
        origin_id: originId,
        created_at: timestamp
      });
      count++;
    }

    // Pertainym (supernova:pertainsTo)
    for (const pertainymId of synset.pertainym || []) {
      const pertainymEntity = `supernova:${typeInfo.prefix}${pertainymId.replace(/[^a-zA-Z0-9]/g, '_')}`;
      triples.push({
        subject: entityIRI,
        predicate: 'supernova:pertainsTo',
        object: pertainymEntity,
        object_value: null,
        object_type: 'iri',
        object_datatype: null,
        tx: tx++,
        origin_id: originId,
        created_at: timestamp
      });
      count++;
    }

    // Examples (skos:example) - as literal values
    for (const example of synset.examples || []) {
      triples.push({
        subject: entityIRI,
        predicate: 'skos:example',
        object: null,
        object_value: example,
        object_type: 'literal',
        object_datatype: 'xsd:string',
        tx: tx++,
        origin_id: originId,
        created_at: timestamp
      });
      count++;
    }
  }

  insert(triples);
  return count;
}

/**
 * Parse WordNet synsets from Turtle file (all types: n, v, a, r)
 * Single-pass with index building: O(n) complexity
 */
async function parseWordNetSynsets(filePath, limits) {
  console.log('   Parsing WordNet file (single pass with indexing)...');

  const fileStream = createReadStream(filePath);
  const rl = createInterface({ input: fileStream, crlfDelay: Infinity });

  // Indexes built in single pass (per type)
  const synsetData = new Map(); // synsetId+type → { definition, hypernyms, type, relations, examples }
  const synsetLemmas = new Map(); // synsetId+type → [lemmas]
  const synsetOrder = { n: [], v: [], a: [], r: [] }; // Track order per type

  // Parser state
  let currentSynset = null;
  let currentType = null;
  let inDefinition = false;
  let inExample = false;
  let currentLemma = null;
  let currentLemmaType = null;
  let currentWrittenRep = null;
  let currentWrittenRepType = null;

  for await (const line of rl) {
    const trimmed = line.trim();

    // === SYNSET BLOCKS ===
    // Start of synset definition (match all types: n, v, a, r)
    if (trimmed.startsWith('wnid:oewn-')) {
      const match = trimmed.match(/wnid:(oewn-\d+)-([nvar])\b/);
      if (match) {
        currentSynset = match[1];
        currentType = match[2];
        const key = `${currentSynset}-${currentType}`;
        synsetOrder[currentType].push(currentSynset);
        synsetData.set(key, {
          definition: null,
          hypernyms: [],
          type: currentType,
          similar: [],
          also: [],
          antonym: [],
          entails: [],
          causes: [],
          mero_part: [],
          holo_part: [],
          mero_member: [],
          holo_member: [],
          mero_substance: [],
          holo_substance: [],
          domain_topic: [],
          pertainym: [],
          examples: []
        });
        inDefinition = false;
      } else {
        currentSynset = null;
        currentType = null;
      }
    }

    // Definition (inside synset block)
    if (currentSynset && currentType) {
      if (trimmed.includes('wn:definition [')) {
        inDefinition = true;
      }
      if (inDefinition && trimmed.includes('rdf:value')) {
        const defMatch = trimmed.match(/"([^"]+)"@en/);
        if (defMatch) {
          const key = `${currentSynset}-${currentType}`;
          synsetData.get(key).definition = defMatch[1];
          inDefinition = false;
        }
      }

      // Examples (inside synset block)
      if (trimmed.includes('wn:example [')) {
        inExample = true;
      }
      if (inExample && trimmed.includes('rdf:value')) {
        const exampleMatch = trimmed.match(/"([^"]+)"@en/);
        if (exampleMatch) {
          const key = `${currentSynset}-${currentType}`;
          synsetData.get(key).examples.push(exampleMatch[1]);
          inExample = false;
        }
      }
      const key = `${currentSynset}-${currentType}`;
      const data = synsetData.get(key);

      // Hypernyms (match same type)
      if (trimmed.includes('wn:hypernym')) {
        const hypMatch = trimmed.match(/wnid:(oewn-\d+)-([nvar])/);
        if (hypMatch && hypMatch[2] === currentType) {
          data.hypernyms.push(hypMatch[1] + '-' + hypMatch[2]);
        }
      }

      // Similar (adjectives, verbs)
      if (trimmed.includes('wn:similar')) {
        const matches = trimmed.matchAll(/wnid:(oewn-\d+)-([nvar])/g);
        for (const match of matches) {
          if (match[2] === currentType) {
            data.similar.push(match[1]);
          }
        }
      }

      // Also (see also)
      if (trimmed.includes('wn:also')) {
        const matches = trimmed.matchAll(/wnid:(oewn-\d+)-([nvar])/g);
        for (const match of matches) {
          if (match[2] === currentType) {
            data.also.push(match[1]);
          }
        }
      }

      // Antonym
      if (trimmed.includes('wn:antonym')) {
        const matches = trimmed.matchAll(/wnid:(oewn-\d+)-([nvar])/g);
        for (const match of matches) {
          if (match[2] === currentType) {
            data.antonym.push(match[1]);
          }
        }
      }

      // Entails (verbs)
      if (trimmed.includes('wn:entails')) {
        const matches = trimmed.matchAll(/wnid:(oewn-\d+)-([nvar])/g);
        for (const match of matches) {
          if (match[2] === currentType) {
            data.entails.push(match[1]);
          }
        }
      }

      // Causes (verbs)
      if (trimmed.includes('wn:causes')) {
        const matches = trimmed.matchAll(/wnid:(oewn-\d+)-([nvar])/g);
        for (const match of matches) {
          if (match[2] === currentType) {
            data.causes.push(match[1]);
          }
        }
      }

      // Meronymy/Holonymy - Part
      if (trimmed.includes('wn:mero_part')) {
        const matches = trimmed.matchAll(/wnid:(oewn-\d+)-([nvar])/g);
        for (const match of matches) {
          if (match[2] === currentType) {
            data.mero_part.push(match[1] + '-' + match[2]);
          }
        }
      }
      if (trimmed.includes('wn:holo_part')) {
        const matches = trimmed.matchAll(/wnid:(oewn-\d+)-([nvar])/g);
        for (const match of matches) {
          if (match[2] === currentType) {
            data.holo_part.push(match[1] + '-' + match[2]);
          }
        }
      }

      // Meronymy/Holonymy - Member
      if (trimmed.includes('wn:mero_member')) {
        const matches = trimmed.matchAll(/wnid:(oewn-\d+)-([nvar])/g);
        for (const match of matches) {
          if (match[2] === currentType) {
            data.mero_member.push(match[1] + '-' + match[2]);
          }
        }
      }
      if (trimmed.includes('wn:holo_member')) {
        const matches = trimmed.matchAll(/wnid:(oewn-\d+)-([nvar])/g);
        for (const match of matches) {
          if (match[2] === currentType) {
            data.holo_member.push(match[1] + '-' + match[2]);
          }
        }
      }

      // Meronymy/Holonymy - Substance
      if (trimmed.includes('wn:mero_substance')) {
        const matches = trimmed.matchAll(/wnid:(oewn-\d+)-([nvar])/g);
        for (const match of matches) {
          if (match[2] === currentType) {
            data.mero_substance.push(match[1] + '-' + match[2]);
          }
        }
      }
      if (trimmed.includes('wn:holo_substance')) {
        const matches = trimmed.matchAll(/wnid:(oewn-\d+)-([nvar])/g);
        for (const match of matches) {
          if (match[2] === currentType) {
            data.holo_substance.push(match[1] + '-' + match[2]);
          }
        }
      }

      // Domain Topic
      if (trimmed.includes('wn:domain_topic')) {
        const matches = trimmed.matchAll(/wnid:(oewn-\d+)-([nvar])/g);
        for (const match of matches) {
          data.domain_topic.push(match[1] + '-' + match[2]);
        }
      }

      // Pertainym (adjectives)
      if (trimmed.includes('wn:pertainym')) {
        const matches = trimmed.matchAll(/wnid:(oewn-\d+)-([nvar])/g);
        for (const match of matches) {
          data.pertainym.push(match[1] + '-' + match[2]);
        }
      }
    }

    // === LEMMA BLOCKS ===
    // Pattern 1: Start of lemma entry - get the lemma word and type
    // <https://en-word.net/lemma/entity#entity-n>
    if (trimmed.startsWith('<https://en-word.net/lemma/')) {
      const match = trimmed.match(/<https:\/\/en-word\.net\/lemma\/([^#]+)#\1-([nvar])>/);
      if (match) {
        currentLemma = match[1];
        currentLemmaType = match[2];
      } else {
        currentLemma = null;
        currentLemmaType = null;
      }
    }

    // Pattern 2: Sense linking - map lemma to synset
    // ontolex:sense <https://en-word.net/lemma/entity#entity-oewn-00001740-n>
    if (currentLemma && currentLemmaType && trimmed.includes('ontolex:sense')) {
      const match = trimmed.match(/#([^-]+-oewn-\d+)-([nvar])>/);
      if (match && match[2] === currentLemmaType) {
        const synsetId = match[1].replace(/^[^-]+-/, '');
        const key = `${synsetId}-${currentLemmaType}`;
        if (!synsetLemmas.has(key)) {
          synsetLemmas.set(key, []);
        }
        synsetLemmas.get(key).push(currentLemma);
      }
    }

    // Pattern 3: WrittenRep block - get actual written form
    // <https://en-word.net/lemma/entity#entity-n-lemma>
    //     ontolex:writtenRep "entity"@en .
    if (trimmed.startsWith('<https://en-word.net/lemma/') && trimmed.includes('-lemma>')) {
      const match = trimmed.match(/<https:\/\/en-word\.net\/lemma\/([^#]+)#\1-([nvar])-lemma>/);
      if (match) {
        currentWrittenRep = match[1];
        currentWrittenRepType = match[2];
      }
    }

    // Replace lemma placeholder with actual written form
    if (currentWrittenRep && currentWrittenRepType && trimmed.includes('ontolex:writtenRep')) {
      const match = trimmed.match(/"([^"]+)"/);
      if (match) {
        const actualWord = match[1];
        // Find and replace all occurrences of placeholder lemma
        for (const [key, lemmas] of synsetLemmas.entries()) {
          if (key.endsWith(`-${currentWrittenRepType}`)) {
            const idx = lemmas.indexOf(currentWrittenRep);
            if (idx !== -1) {
              lemmas[idx] = actualWord;
            }
          }
        }
        currentWrittenRep = null;
        currentWrittenRepType = null;
      }
    }
  }

  // Log indexing results
  console.log(`   Indexed: ${synsetOrder.n.length}n, ${synsetOrder.v.length}v, ${synsetOrder.a.length}a, ${synsetOrder.r.length}r synsets`);

  // Build final synset objects (limit per type)
  const synsets = [];

  for (const [type, limit] of Object.entries(limits)) {
    const order = synsetOrder[type];
    const count = Math.min(limit, order.length);

    for (let i = 0; i < count; i++) {
      const id = order[i];
      const key = `${id}-${type}`;
      const data = synsetData.get(key);
      const lemmas = synsetLemmas.get(key) || [];

      // Skip nouns without hypernyms (orphan concepts = encyclopedic knowledge)
      // EXCEPT 'entity' (oewn-00001740-n) which is the root abstract concept
      if (type === 'n' && data.hypernyms.length === 0 && id !== 'oewn-00001740') {
        continue;
      }

      synsets.push({
        id: `${id}-${type}`,
        type,
        lemmas,
        definition: data.definition,
        hypernyms: data.hypernyms,
        similar: data.similar,
        also: data.also,
        antonym: data.antonym,
        entails: data.entails,
        causes: data.causes,
        mero_part: data.mero_part,
        holo_part: data.holo_part,
        mero_member: data.mero_member,
        holo_member: data.holo_member,
        mero_substance: data.mero_substance,
        holo_substance: data.holo_substance,
        domain_topic: data.domain_topic,
        pertainym: data.pertainym,
        examples: data.examples
      });
    }

    console.log(`   Collected ${count} ${type}-type synsets`);
  }

  const totalLemmas = synsets.reduce((sum, s) => sum + s.lemmas.length, 0);
  console.log(`   Total: ${synsets.length} synsets with ${totalLemmas} lemmas`);
  return synsets;
}
