<script>
	import { invoke } from '@tauri-apps/api/core';
	import { Button } from '$lib/components/ui/button';
	import { Textarea } from '$lib/components/ui/textarea';
	import { Checkbox } from '$lib/components/ui/checkbox';

	let { q, answer = null, isLast, conversationId } = $props();

	function formatAnswer(raw) {
		try {
			const parsed = JSON.parse(raw);
			if (Array.isArray(parsed)) return parsed.join(', ');
		} catch {}
		return raw;
	}

	let selectedOptions = $state([]);
	let textInput = $state('');

	async function answerQuestion(toolUseId, answer) {
		try {
			await invoke('chat__answer_question', { conversationId, toolUseId, answer });
		} catch (err) {
			console.error('[QUESTION] answerQuestion FAILED:', err);
		}
	}

	async function dismissQuestion(toolUseId) {
		try {
			await invoke('chat__dismiss_question', { conversationId, toolUseId });
		} catch (err) {
			console.error('[QUESTION] dismissQuestion FAILED:', err);
		}
	}
</script>

<div class="question-block">
	<div class="question-header">
		<span class="material-symbols-outlined question-icon">help_outline</span>
		<span class="question-label">Question</span>
	</div>
	<div class="question-text">{q.question}</div>
	{#if answer === '[Question dismissed by user]'}
		<div class="question-answered">Cancelled</div>
	{:else if answer}
		<div class="question-answer">{formatAnswer(answer)}</div>
	{:else if isLast}
		{#if q.question_type === 'single'}
			<div class="question-options">
				{#each (q.options ?? []) as option}
					<Button variant="ghost" class="option-btn" onclick={() => answerQuestion(q.id, option)}>{option}</Button>
				{/each}
			</div>
		{:else if q.question_type === 'multi'}
			<div class="question-options">
				{#each (q.options ?? []) as option}
					<label class="option-checkbox-label">
						<Checkbox checked={selectedOptions.includes(option)} onCheckedChange={(checked) => {
							if (checked) selectedOptions = [...selectedOptions, option];
							else selectedOptions = selectedOptions.filter(o => o !== option);
						}} />
						{option}
					</label>
				{/each}
			</div>
			<Button variant="ghost" class="question-submit" onclick={() => answerQuestion(q.id, selectedOptions)}>Submit</Button>
		{:else}
			<Textarea bind:value={textInput} placeholder="Type your answer..." class="question-textarea" />
			<Button variant="ghost" class="question-submit" onclick={() => answerQuestion(q.id, textInput)}>Submit</Button>
		{/if}
		<Button variant="ghost" class="question-dismiss" onclick={() => dismissQuestion(q.id)}>Cancel</Button>
	{:else}
		<div class="question-answered">Answered</div>
	{/if}
</div>

<style>
	.question-block {
		display: flex;
		flex-direction: column;
		gap: 8px;
		padding: 10px 12px;
		background: var(--muted);
		margin-top: 4px;
	}

	.question-header {
		display: flex;
		align-items: center;
		gap: 5px;
	}

	.question-icon {
		font-size: 14px;
		color: var(--muted-foreground);
	}

	.question-label {
		font-size: 10px;
		font-weight: 600;
		letter-spacing: 0.08em;
		text-transform: uppercase;
		color: var(--muted-foreground);
	}

	.question-text {
		font-size: 14px;
		color: var(--accent-foreground);
		line-height: 1.5;
	}

	.question-options {
		display: flex;
		flex-direction: column;
		gap: 4px;
		align-items: flex-start;
	}

	:global(.option-btn) {
		text-align: left !important;
		justify-content: flex-start !important;
		font-size: 12px !important;
		height: auto !important;
		padding: 6px 10px !important;
		background: color-mix(in srgb, var(--primary) 8%, transparent) !important;
		color: var(--accent-foreground) !important;
		width: 100% !important;
	}

	.option-checkbox-label {
		display: flex;
		align-items: center;
		gap: 8px;
		font-size: 12px;
		color: var(--accent-foreground);
		cursor: pointer;
		padding: 4px 0;
	}

	:global(.option-checkbox) {
		width: 14px;
		height: 14px;
		cursor: pointer;
		flex-shrink: 0;
	}

	:global(.question-textarea) {
		min-height: 72px !important;
		font-size: 12px !important;
	}

	:global(.question-submit) {
		align-self: flex-end;
		font-size: 12px !important;
		font-weight: 500 !important;
		color: var(--primary) !important;
	}

	.question-answered {
		font-size: 11px;
		color: var(--muted-foreground);
		font-style: italic;
		opacity: 0.6;
	}

	.question-answer {
		font-size: 14px;
		color: var(--accent-foreground);
		padding: 6px 10px;
		background: var(--input);
	}

	:global(.question-dismiss) {
		align-self: flex-end;
		color: var(--muted-foreground) !important;
		font-size: 11px !important;
	}
</style>
