type TimestampInput = Date | number | string;

export function getSystemLocale(): string {
	if (typeof navigator !== 'undefined' && navigator.language) return navigator.language;
	return Intl.DateTimeFormat().resolvedOptions().locale;
}

function toDate(value: TimestampInput): Date {
	if (value instanceof Date) return value;
	if (typeof value === 'string' && /^\d+$/.test(value)) return new Date(Number(value));
	if (typeof value === 'string') {
		const dateOnly = /^(\d{4})-(\d{2})-(\d{2})$/.exec(value);
		if (dateOnly) return new Date(Number(dateOnly[1]), Number(dateOnly[2]) - 1, Number(dateOnly[3]), 12);
	}
	return new Date(value);
}

export function formatRelativeCalendarDate(
	value: TimestampInput,
	referenceTime = Date.now(),
	style: Intl.RelativeTimeFormatStyle = 'long'
): string {
	const date = toDate(value);
	const reference = new Date(referenceTime);
	if (!Number.isFinite(date.getTime()) || !Number.isFinite(reference.getTime())) return '';
	const calendarDay = (item: Date) => Date.UTC(item.getFullYear(), item.getMonth(), item.getDate());
	const days = Math.round((calendarDay(date) - calendarDay(reference)) / 86400000);
	return new Intl.RelativeTimeFormat(getSystemLocale(), { numeric: 'auto', style }).format(days, 'day');
}

export function formatRelativeTime(
	value: TimestampInput,
	referenceTime = Date.now(),
	style: Intl.RelativeTimeFormatStyle = 'long'
): string {
	const timestamp = toDate(value).getTime();
	if (!Number.isFinite(timestamp)) return '';

	const deltaSeconds = (timestamp - referenceTime) / 1000;
	const absoluteSeconds = Math.abs(deltaSeconds);
	let unit: Intl.RelativeTimeFormatUnit;
	let amount: number;

	if (absoluteSeconds < 45) {
		unit = 'second';
		amount = Math.round(deltaSeconds);
	} else if (absoluteSeconds < 45 * 60) {
		unit = 'minute';
		amount = Math.round(deltaSeconds / 60);
	} else if (absoluteSeconds < 22 * 60 * 60) {
		unit = 'hour';
		amount = Math.round(deltaSeconds / 3600);
	} else if (absoluteSeconds < 26 * 86400) {
		unit = 'day';
		amount = Math.round(deltaSeconds / 86400);
	} else if (absoluteSeconds < 320 * 86400) {
		unit = 'month';
		amount = Math.round(deltaSeconds / (30 * 86400));
	} else {
		unit = 'year';
		amount = Math.round(deltaSeconds / (365 * 86400));
	}

	return new Intl.RelativeTimeFormat(getSystemLocale(), { numeric: 'auto', style }).format(amount, unit);
}

export function formatLocalizedDate(value: TimestampInput, options?: Intl.DateTimeFormatOptions): string {
	const date = toDate(value);
	if (!Number.isFinite(date.getTime())) return '';
	return new Intl.DateTimeFormat(getSystemLocale(), options).format(date);
}

export function formatLocalizedDateTime(value: TimestampInput): string {
	const date = toDate(value);
	if (!Number.isFinite(date.getTime())) return '';
	return new Intl.DateTimeFormat(getSystemLocale(), { dateStyle: 'medium', timeStyle: 'short' }).format(date);
}
