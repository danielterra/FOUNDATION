export function matchesChangedPredicates(changedPredicates: string[] | undefined, watchedPredicates: string[]) {
  if (!changedPredicates?.length) return true;

  const watched = new Set(watchedPredicates);
  if (changedPredicates.some((predicate) => watched.has(predicate))) return true;

  const localName = (iri: string) => iri.split(/[\/#:]/).filter(Boolean).at(-1);
  const watchedNames = new Set(watchedPredicates.map(localName));
  return changedPredicates.some((predicate) => watchedNames.has(localName(predicate)));
}
