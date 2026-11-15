export type ComparisonResult = {
  current: number;
  reference: number;
  delta: number;
  percentChange: number | null;
  direction: 'higher' | 'lower' | 'unchanged';
  interpretation: 'favorable' | 'unfavorable' | 'neutral';
};

function parseFiniteNumber(value: string | number): number | null {
  if (typeof value === 'string' && value.trim() === '') return null;
  const parsed = Number(value);
  return Number.isFinite(parsed) ? parsed : null;
}

export function areComparisonUnitsCompatible(
  currentUnitIri?: string | null,
  currentUnitLabel?: string | null,
  referenceUnitIri?: string | null,
  referenceUnitLabel?: string | null,
) {
  const currentUnit = currentUnitIri ?? currentUnitLabel ?? null;
  const referenceUnit = referenceUnitIri ?? referenceUnitLabel ?? null;
  return currentUnit === referenceUnit;
}

export function canCalculateComparison(
  periodClosed: boolean,
  currentValue: string | number | null | undefined,
  referenceValue: string | number | null | undefined,
  unitsCompatible: boolean,
) {
  if (!periodClosed || !unitsCompatible || currentValue == null || referenceValue == null) return false;
  const current = parseFiniteNumber(currentValue);
  const reference = parseFiniteNumber(referenceValue);
  return current !== null && reference !== null && reference !== 0;
}

export function calculateComparison(
  currentValue: string | number,
  referenceValue: string | number,
  favorableDirection: 'higher' | 'lower' | 'neutral',
): ComparisonResult | null {
  const current = parseFiniteNumber(currentValue);
  const reference = parseFiniteNumber(referenceValue);
  if (current === null || reference === null || reference === 0) return null;

  const delta = current - reference;
  const direction = delta > 0 ? 'higher' : delta < 0 ? 'lower' : 'unchanged';
  const interpretation = favorableDirection === 'neutral' || direction === 'unchanged'
    ? 'neutral'
    : direction === favorableDirection ? 'favorable' : 'unfavorable';

  return {
    current,
    reference,
    delta,
    percentChange: (delta / Math.abs(reference)) * 100,
    direction,
    interpretation,
  };
}

export function comparisonBarGeometry(current: number, reference: number) {
  const minimum = Math.min(0, current, reference);
  const maximum = Math.max(0, current, reference);
  const span = maximum - minimum;
  if (span === 0) return null;

  const zero = ((0 - minimum) / span) * 100;
  const geometry = (amount: number) => {
    const edge = ((amount - minimum) / span) * 100;
    return { start: Math.min(zero, edge), width: Math.abs(edge - zero) };
  };

  return { current: geometry(current), reference: geometry(reference), zero };
}

export function formatSemanticAmount(value: number, unitLabel?: string | null, locale = Intl.DateTimeFormat().resolvedOptions().locale): string {
  if (unitLabel && /^[A-Z]{3}$/.test(unitLabel) && Intl.supportedValuesOf('currency').includes(unitLabel)) {
    return new Intl.NumberFormat(locale, { style: 'currency', currency: unitLabel }).format(value);
  }
  const formatted = new Intl.NumberFormat(locale, { maximumFractionDigits: 2 }).format(value);
  return unitLabel ? `${formatted} ${unitLabel}` : formatted;
}

export function formatSemanticPercent(value: number | null, locale = Intl.DateTimeFormat().resolvedOptions().locale) {
  if (value === null) return null;
  return new Intl.NumberFormat(locale, { style: 'percent', maximumFractionDigits: 1 }).format(value / 100);
}

export function isComparisonPeriodClosed(endValue: string | null | undefined, now = new Date()): boolean {
  if (!endValue) return false;
  const match = /^(\d{4})-(\d{2})-(\d{2})/.exec(endValue);
  if (!match) return false;
  const [, year, month, day] = match;
  const end = new Date(Number(year), Number(month) - 1, Number(day));
  const today = new Date(now.getFullYear(), now.getMonth(), now.getDate());
  return end < today;
}

export function formatComparisonMonth(startValue: string | null | undefined, locale = Intl.DateTimeFormat().resolvedOptions().locale): string | null {
  if (!startValue) return null;
  const match = /^(\d{4})-(\d{2})-(\d{2})/.exec(startValue);
  if (!match) return null;
  const [, year, month, day] = match;
  const date = new Date(Number(year), Number(month) - 1, Number(day));
  return new Intl.DateTimeFormat(locale, { month: 'long', year: 'numeric' }).format(date);
}

export function previousComparisonMonth(startValue: string | null | undefined, locale = Intl.DateTimeFormat().resolvedOptions().locale): string | null {
  if (!startValue) return null;
  const match = /^(\d{4})-(\d{2})-(\d{2})/.exec(startValue);
  if (!match) return null;
  const [, year, month] = match;
  const date = new Date(Number(year), Number(month) - 2, 1);
  return new Intl.DateTimeFormat(locale, { month: 'long', year: 'numeric' }).format(date);
}
