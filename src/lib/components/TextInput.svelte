<script>
	import { onMount } from 'svelte';

	let {
		value = $bindable(''),
		placeholder = '',
		type = 'text',
		autofocus = false,
		disabled = false,
		onkeydown,
		oninput,
		onfocus,
		onblur
	} = $props();

	let inputElement;

	onMount(() => {
		if (autofocus && inputElement) {
			inputElement.focus();
		}
	});
</script>

<input
	bind:this={inputElement}
	{type}
	bind:value
	{placeholder}
	{disabled}
	onkeydown={onkeydown}
	oninput={oninput}
	onfocus={onfocus}
	onblur={onblur}
/>

<style>
	input {
		width: 100%;
		padding: 1rem 1.25rem;
		background: var(--color-black);
		border: 1px solid color-mix(in srgb, var(--color-white) 10%, transparent);
		border-radius: 8px;
		color: var(--color-neutral-active);
		font-size: 1.125rem;
		text-align: center;
		transition: all 0.2s;
	}

	input:focus {
		outline: none;
		border-color: var(--color-interactive);
		background: var(--color-black);
	}

	input:disabled {
		opacity: 0.5;
		cursor: not-allowed;
	}

	input::placeholder {
		color: color-mix(in srgb, var(--color-white) 30%, transparent);
	}
</style>
