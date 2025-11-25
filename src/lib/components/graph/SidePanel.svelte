<script>
	export let currentNodeLabel = '';
	export let nodeFacts = [];
	export let loadingFacts = false;
	export let onNavigateToNode;
	export let getNodeDisplayName;
</script>

<aside class="floating-side-panel">
	<div class="panel-header">
		<h3>{currentNodeLabel}</h3>
		<span class="fact-count">{nodeFacts.length} facts</span>
	</div>

	{#if loadingFacts}
		<div class="loading-state">Loading facts...</div>
	{:else if nodeFacts.length === 0}
		<div class="empty-state">No facts available</div>
	{:else}
		<div class="facts-form">
			{#each nodeFacts as fact}
				<div class="form-row">
					<label class="form-label" title={fact.a}>
						{fact.a.split(/[/#]/).pop()}
					</label>
					<div class="form-value">
						{#if fact.v_type === 'ref'}
							<button class="entity-link" on:click={() => onNavigateToNode(fact.v)}>
								{getNodeDisplayName(fact.v)}
							</button>
						{:else}
							<span class="literal-value">{fact.v}</span>
						{/if}
					</div>
				</div>
			{/each}
		</div>
	{/if}
</aside>

<style>
	.floating-side-panel {
		position: fixed;
		top: 80px;
		right: 20px;
		width: 380px;
		max-height: calc(100vh - 100px);
		background: rgba(30, 30, 30, 0.95);
		backdrop-filter: blur(10px);
		border: 1px solid rgba(255, 255, 255, 0.1);
		border-radius: 12px;
		box-shadow: 0 4px 20px rgba(0, 0, 0, 0.5);
		z-index: 999;
		display: flex;
		flex-direction: column;
		overflow: hidden;
	}

	.panel-header {
		padding: 20px 24px;
		border-bottom: 1px solid rgba(255, 255, 255, 0.1);
		display: flex;
		justify-content: space-between;
		align-items: center;
		flex-shrink: 0;
	}

	.panel-header h3 {
		margin: 0;
		font-family: 'Science Gothic Medium', 'Science Gothic', sans-serif;
		font-size: 16px;
		font-weight: 500;
		color: #ffffff;
		flex: 1;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.fact-count {
		font-family: 'Science Gothic SemiCondensed Light', 'Science Gothic', sans-serif;
		font-size: 12px;
		color: rgba(255, 140, 66, 0.8);
		background: rgba(255, 140, 66, 0.1);
		padding: 4px 10px;
		border-radius: 6px;
		margin-left: 12px;
		flex-shrink: 0;
	}

	.facts-form {
		padding: 16px 24px;
		overflow-y: auto;
		flex: 1;
	}

	.form-row {
		margin-bottom: 16px;
	}

	.form-label {
		display: block;
		font-family: 'Science Gothic SemiCondensed Light', 'Science Gothic', sans-serif;
		font-size: 11px;
		text-transform: uppercase;
		letter-spacing: 0.5px;
		color: rgba(255, 255, 255, 0.5);
		margin-bottom: 6px;
	}

	.form-value {
		font-family: 'Science Gothic SemiCondensed Light', 'Science Gothic', sans-serif;
		font-size: 14px;
		color: rgba(255, 255, 255, 0.9);
		word-break: break-word;
	}

	.entity-link {
		background: none;
		border: none;
		color: #ff8c42;
		cursor: pointer;
		text-decoration: none;
		padding: 0;
		font-family: inherit;
		font-size: inherit;
		text-align: left;
		transition: color 0.2s;
	}

	.entity-link:hover {
		color: #ffb380;
		text-decoration: underline;
	}

	.literal-value {
		color: rgba(255, 255, 255, 0.8);
	}

	.loading-state,
	.empty-state {
		padding: 40px 24px;
		text-align: center;
		font-family: 'Science Gothic SemiCondensed Light', 'Science Gothic', sans-serif;
		font-size: 14px;
		color: rgba(255, 255, 255, 0.5);
	}

	/* Custom scrollbar */
	.facts-form::-webkit-scrollbar {
		width: 8px;
	}

	.facts-form::-webkit-scrollbar-track {
		background: rgba(0, 0, 0, 0.2);
		border-radius: 4px;
	}

	.facts-form::-webkit-scrollbar-thumb {
		background: rgba(255, 140, 66, 0.3);
		border-radius: 4px;
	}

	.facts-form::-webkit-scrollbar-thumb:hover {
		background: rgba(255, 140, 66, 0.5);
	}
</style>
