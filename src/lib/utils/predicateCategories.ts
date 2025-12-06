// Predicate categories for organizing triples
export const hierarchicalPredicates = [
	'http://www.w3.org/2000/01/rdf-schema#subClassOf',
	'http://www.w3.org/2000/01/rdf-schema#subPropertyOf',
	'http://www.w3.org/2004/02/skos/core#broader'
];

export const semanticPredicates = [
	'http://www.w3.org/2004/02/skos/core#related',
	'http://FOUNDATION.local/ontology/antonym',
	'http://www.w3.org/2000/01/rdf-schema#seeAlso',
	'http://FOUNDATION.local/ontology/causes',
	'http://FOUNDATION.local/ontology/entails',
	'http://FOUNDATION.local/ontology/partOf',
	'http://FOUNDATION.local/ontology/hasPart',
	'http://FOUNDATION.local/ontology/memberOf',
	'http://FOUNDATION.local/ontology/hasMember',
	'http://FOUNDATION.local/ontology/madeOf',
	'http://FOUNDATION.local/ontology/containsSubstance',
	'http://FOUNDATION.local/ontology/domainTopic',
	'http://FOUNDATION.local/ontology/pertainsTo'
];
