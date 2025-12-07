// Predicate categories for organizing triples
export const hierarchicalPredicates = [
	'http://www.w3.org/2000/01/rdf-schema#subClassOf',
	'http://www.w3.org/2000/01/rdf-schema#subPropertyOf',
	'http://www.w3.org/2004/02/skos/core#broader'
];

export const semanticPredicates = [
	'http://www.w3.org/2004/02/skos/core#related',
	'http://foundation.local/ontology/antonym',
	'http://www.w3.org/2000/01/rdf-schema#seeAlso',
	'http://foundation.local/ontology/causes',
	'http://foundation.local/ontology/entails',
	'http://foundation.local/ontology/partOf',
	'http://foundation.local/ontology/hasPart',
	'http://foundation.local/ontology/memberOf',
	'http://foundation.local/ontology/hasMember',
	'http://foundation.local/ontology/madeOf',
	'http://foundation.local/ontology/containsSubstance',
	'http://foundation.local/ontology/domainTopic',
	'http://foundation.local/ontology/pertainsTo'
];
