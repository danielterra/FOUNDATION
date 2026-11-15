import assert from 'node:assert/strict';
import { test } from 'node:test';
import { createJiti } from 'jiti';

const jiti = createJiti(import.meta.url);
const { matchesChangedPredicates } = (await jiti.import('./entityPredicates.ts')) as typeof import('./entityPredicates.ts');

test('refreshes for a bound compact predicate', () => {
  assert.equal(matchesChangedPredicates(['foundation:result'], ['foundation:result']), true);
});

test('matches a bound predicate when the event uses an expanded IRI', () => {
  assert.equal(matchesChangedPredicates(['https://example.test/foundation#result'], ['foundation:result']), true);
});

test('refreshes when an update event does not identify changed predicates', () => {
  assert.equal(matchesChangedPredicates([], ['foundation:result']), true);
  assert.equal(matchesChangedPredicates(undefined, ['foundation:result']), true);
});

test('ignores an unrelated predicate', () => {
  assert.equal(matchesChangedPredicates(['foundation:description'], ['foundation:result']), false);
});
