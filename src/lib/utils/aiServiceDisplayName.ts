type AIServiceLabelSource = {
	label?: string | null;
	description?: string | null;
	seeAlsoUrl?: string | null;
};

export function aiServiceDisplayName(service: AIServiceLabelSource): string {
	const label = service.label?.trim() ?? '';
	if (label && !/^(foundation|anthropic):/i.test(label)) return label;

	const descriptionPrefix = service.description?.split(/\s+[—–]\s+/u, 1)[0]?.trim() ?? '';
	const providerName = descriptionPrefix.match(/^(.+?)\s+(?:AI|API)\s+gateway$/iu)?.[1]?.trim();
	if (providerName || descriptionPrefix) return providerName || descriptionPrefix;

	const officialUrl = service.seeAlsoUrl?.trim() ?? '';
	const officialHost = officialUrl.match(/^https?:\/\/(?:www\.)?([^/:?#]+)/iu)?.[1]?.toLowerCase() ?? '';
	if (officialHost === 'openrouter.ai') return 'OpenRouter';
	if (officialHost === 'anthropic.com') return 'Anthropic';
	return 'Serviço de IA';
}
