// Formatting utility functions for the side panel

/**
 * Extract the label from a predicate URI
 */
export function getPredicateLabel(predicate: string): string {
	return predicate.split(/[/#]/).pop() || predicate;
}

/**
 * Detect icon type (image URL vs material symbol)
 */
export function getIconType(icon: string | null): 'image' | 'material-symbol' | null {
	if (!icon) return null;
	if (icon.startsWith('http://') || icon.startsWith('https://') ||
	    icon.startsWith('file://') || icon.startsWith('data:')) {
		return 'image';
	}
	return 'material-symbol';
}

/**
 * Map XSD datatypes to appropriate Material Symbols icons
 */
export function getDatatypeIcon(rangeLabel: string | null | undefined): string {
	if (!rangeLabel) return 'text_fields';

	const label = rangeLabel.toLowerCase();

	// String types
	if (label.includes('string') || label.includes('literal')) return 'text_fields';

	// Numeric types
	if (label.includes('integer') || label.includes('int') || label.includes('long') ||
	    label.includes('short') || label.includes('byte')) return '123';
	if (label.includes('decimal') || label.includes('float') || label.includes('double')) return 'decimal';

	// Boolean
	if (label.includes('boolean')) return 'toggle_on';

	// Date/Time types
	if (label.includes('datetime')) return 'calendar_clock';
	if (label.includes('date')) return 'calendar_today';
	if (label.includes('time')) return 'schedule';

	// URI/URL
	if (label.includes('uri') || label.includes('url') || label.includes('anyuri')) return 'link';

	// Binary/Data
	if (label.includes('base64') || label.includes('hexbinary')) return 'data_object';

	// Default
	return 'text_fields';
}
