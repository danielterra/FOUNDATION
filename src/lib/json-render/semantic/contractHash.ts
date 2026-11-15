export function buildSemanticContractHashPayload(
  componentId: string,
  intent: string,
  propsSchema: Record<string, unknown>,
  slots: readonly unknown[],
  actionsSchema: unknown,
  examples: unknown,
) {
  const contractPropsSchema = { ...propsSchema };
  if (slots.length > 0) contractPropsSchema['x-foundation-slots'] = slots;

  return { componentId, intent, propsSchema: contractPropsSchema, actionsSchema, examples };
}

export function stableJson(value: unknown): string {
  if (Array.isArray(value)) return `[${value.map(stableJson).join(',')}]`;
  if (value && typeof value === 'object') {
    const entries = Object.entries(value as Record<string, unknown>).sort(([left], [right]) => left < right ? -1 : left > right ? 1 : 0);
    return `{${entries.map(([key, item]) => `${JSON.stringify(key)}:${stableJson(item)}`).join(',')}}`;
  }
  return JSON.stringify(value);
}

export async function hashSemanticContract(payload: unknown): Promise<string> {
  const digest = await globalThis.crypto.subtle.digest('SHA-256', new TextEncoder().encode(stableJson(payload)));
  const hash = [...new Uint8Array(digest)].map((byte) => byte.toString(16).padStart(2, '0')).join('');
  return `sha256:${hash}`;
}
