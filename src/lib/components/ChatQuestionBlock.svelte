<script>
	import { invoke } from '@tauri-apps/api/core';

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
	{#if isLast && !answer}
		{#if q.question_type === 'single'}
			<div class="question-options">
				{#each (q.options ?? []) as option}
					<button class="option-btn" onclick={() => answerQuestion(q.id, option)}>{option}</button>
				{/each}
			</div>
		{:else if q.question_type === 'multi'}
			<div class="question-options">
				{#each (q.options ?? []) as option}
					<label class="option-checkbox-label">
						<input class="option-checkbox" type="checkbox" bind:group={selectedOptions} value={option} />
						{option}
					</label>
				{/each}
			</div>
			<button class="question-submit" onclick={() => answerQuestion(q.id, selectedOptions)}>Submit</button>
		{:else}
			<textarea class="question-textarea" bind:value={textInput} placeholder="Type your answer..."></textarea>
			<button class="question-submit" onclick={() => answerQuestion(q.id, textInput)}>Submit</button>
		{/if}
		<button class="question-dismiss" onclick={() => dismissQuestion(q.id)}>Cancel</button>
	{:else if answer === '[Question dismissed by user]'}
		<div class="question-answered">Cancelled</div>
	{:else if answer}
		<div class="question-answer">{formatAnswer(answer)}</div>
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
		background: color-mix(in srgb, var(--color-neutral-active) 6%, transparent);
		margin-top: 4px;
	}

	.question-header {
		display: flex;
		align-items: center;
		gap: 5px;
	}

	.question-icon {
		font-size: 14px;
		color: var(--color-neutral-disabled);
	}

	.question-label {
		font-size: 10px;
		font-weight: 600;
		letter-spacing: 0.08em;
		text-transform: uppercase;
		color: var(--color-neutral-disabled);
	}

	.question-text {
		font-size: 14px;
		color: var(--color-neutral-active);
		line-height: 1.5;
	}

	.question-options {
		display: flex;
		flex-direction: column;
		gap: 4px;
	}

	.option-btn {
		text-align: left;
		padding: 6px 10px;
		border: none;
		background: color-mix(in srgb, var(--color-interactive) 8%, transparent);
		color: var(--color-neutral-active);
		font-size: 12px;
		cursor: pointer;
		transition: background 0.15s;
	}

	.option-btn:active {
		background: color-mix(in srgb, var(--color-interactive) 20%, transparent);
	}

	.option-checkbox-label {
		display: flex;
		align-items: center;
		gap: 8px;
		font-size: 12px;
		color: var(--color-neutral-active);
		cursor: pointer;
		padding: 4px 0;
	}

	.option-checkbox {
		accent-color: var(--color-interactive);
		width: 14px;
		height: 14px;
		cursor: pointer;
		flex-shrink: 0;
	}

	.question-textarea {
		width: 100%;
		min-height: 72px;
		padding: 8px 10px;
		border: none;
		background: color-mix(in srgb, var(--color-black) 20%, transparent);
		color: var(--color-neutral-active);
		font-size: 12px;
		font-family: inherit;
		resize: vertical;
		box-sizing: border-box;
		outline: none;
	}

	.question-submit {
		align-self: flex-end;
		padding: 5px 14px;
		border: none;
		background: color-mix(in srgb, var(--color-interactive) 15%, transparent);
		color: var(--color-interactive);
		font-size: 12px;
		font-weight: 500;
		cursor: pointer;
		transition: background 0.15s;
	}

	.question-submit:active {
		background: color-mix(in srgb, var(--color-interactive) 25%, transparent);
	}

	.question-answered {
		font-size: 11px;
		color: var(--color-neutral-disabled);
		font-style: italic;
		opacity: 0.6;
	}

	.question-answer {
		font-size: 14px;
		color: var(--color-neutral-active);
		padding: 6px 10px;
		background: color-mix(in srgb, var(--color-neutral-active) 8%, transparent);
	}

	.question-dismiss {
		align-self: flex-end;
		background: transparent;
		border: none;
		color: var(--color-neutral-disabled);
		padding: 4px 10px;
		font-size: 11px;
		cursor: pointer;
	}

	.question-dismiss:active {
		background: color-mix(in srgb, var(--color-neutral-disabled) 15%, transparent);
	}
</style>
