<script>
	export let onRecenter;
</script>

<div class="floating-top-bar">
	<div class="left-controls">
		<header class="floating-header">
			<h1>SuperNOVA</h1>
			<span class="screen-indicator">Ontology Graph</span>
		</header>

		<button class="recenter-btn" on:click={onRecenter} title="Recenter graph (⌘0)">
			<svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor">
				<circle cx="12" cy="12" r="10" stroke-width="2" />
				<circle cx="12" cy="12" r="3" fill="currentColor" />
			</svg>
		</button>
	</div>

	<div class="floating-info-pill">
		<div class="legend">
			<div class="legend-item">
				<div class="legend-color legend-color-1"></div>
				<span>RDF/RDFS/OWL</span>
			</div>
			<div class="legend-item">
				<div class="legend-color legend-color-2"></div>
				<span>BFO</span>
			</div>
			<div class="legend-item">
				<div class="legend-color legend-color-3"></div>
				<span>CCO</span>
			</div>
		</div>
	</div>
</div>

<style>
	.floating-top-bar {
		position: fixed;
		top: 20px;
		left: 20px;
		right: 20px;
		display: flex;
		justify-content: space-between;
		align-items: center;
		z-index: 1000;
		pointer-events: none;
	}

	.left-controls {
		display: flex;
		align-items: center;
		gap: 16px;
	}

	.floating-header {
		background: rgba(30, 30, 30, 0.95);
		backdrop-filter: blur(10px);
		padding: 12px 20px;
		border-radius: 12px;
		border: 1px solid rgba(255, 255, 255, 0.1);
		box-shadow: 0 4px 20px rgba(0, 0, 0, 0.5);
		display: flex;
		align-items: center;
		gap: 12px;
		pointer-events: auto;
	}

	.floating-header h1 {
		margin: 0;
		font-family: 'Science Gothic Medium', 'Science Gothic', sans-serif;
		font-size: 20px;
		font-weight: 500;
		color: #ffffff;
		letter-spacing: 0.5px;
	}

	.screen-indicator {
		font-family: 'Science Gothic SemiCondensed Light', 'Science Gothic', sans-serif;
		font-size: 14px;
		color: rgba(255, 255, 255, 0.6);
		padding-left: 12px;
		border-left: 1px solid rgba(255, 255, 255, 0.15);
	}

	.recenter-btn {
		background: rgba(30, 30, 30, 0.95);
		backdrop-filter: blur(10px);
		border: 1px solid rgba(255, 255, 255, 0.1);
		border-radius: 12px;
		padding: 12px;
		cursor: pointer;
		color: #ff8c42;
		transition: all 0.2s;
		pointer-events: auto;
		display: flex;
		align-items: center;
		justify-content: center;
	}

	.recenter-btn:hover {
		background: rgba(255, 140, 66, 0.1);
		border-color: #ff8c42;
		transform: scale(1.05);
	}

	.floating-info-pill {
		background: rgba(30, 30, 30, 0.95);
		backdrop-filter: blur(10px);
		padding: 12px 20px;
		border-radius: 12px;
		border: 1px solid rgba(255, 255, 255, 0.1);
		box-shadow: 0 4px 20px rgba(0, 0, 0, 0.5);
		pointer-events: auto;
	}

	.legend {
		display: flex;
		gap: 24px;
		font-family: 'Science Gothic SemiCondensed Light', 'Science Gothic', sans-serif;
		font-size: 13px;
	}

	.legend-item {
		display: flex;
		align-items: center;
		gap: 8px;
		color: rgba(255, 255, 255, 0.8);
	}

	.legend-color {
		width: 12px;
		height: 12px;
		border-radius: 3px;
		border: 1px solid rgba(255, 255, 255, 0.2);
	}

	.legend-color-1 {
		background: #4a9eff;
	}

	.legend-color-2 {
		background: #ff6b9d;
	}

	.legend-color-3 {
		background: #50fa7b;
	}
</style>
