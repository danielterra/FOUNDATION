export function resolveDecisionPresentation(candidateCount: number, complexity: 'simple' | 'complex') {
  return {
    searchable: candidateCount > 4,
    comparison: complexity === 'complex',
  } as const;
}
