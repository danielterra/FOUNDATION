<script>
	let {
		children,
		onZoomChange = null
	} = $props();

	let canvasContainer;
	let transform = $state({ x: 0, y: 0, scale: 1 });
	let isPanning = $state(false);
	let panStart = $state({ x: 0, y: 0 });
	let transformStart = $state({ x: 0, y: 0 });

	const MIN_ZOOM = 0.1;
	const MAX_ZOOM = 3;
	const ZOOM_SENSITIVITY = 0.001;

	function handleWheel(event) {
		event.preventDefault();

		if (event.ctrlKey || event.metaKey) {
			// Zoom with Ctrl/Cmd + Wheel
			const delta = -event.deltaY * ZOOM_SENSITIVITY;
			const newScale = Math.min(MAX_ZOOM, Math.max(MIN_ZOOM, transform.scale * (1 + delta)));

			// Zoom towards mouse position
			const rect = canvasContainer.getBoundingClientRect();
			const mouseX = event.clientX - rect.left;
			const mouseY = event.clientY - rect.top;

			const scaleChange = newScale / transform.scale;
			const newX = mouseX - (mouseX - transform.x) * scaleChange;
			const newY = mouseY - (mouseY - transform.y) * scaleChange;

			transform = { x: newX, y: newY, scale: newScale };

			if (onZoomChange) {
				onZoomChange(newScale);
			}
		} else {
			// Pan with regular wheel
			transform = {
				...transform,
				x: transform.x - event.deltaX,
				y: transform.y - event.deltaY
			};
		}
	}

	function handleMouseDown(event) {
		// Only pan with middle mouse or space+left click
		if (event.button === 1 || (event.button === 0 && event.shiftKey)) {
			event.preventDefault();
			isPanning = true;
			panStart = { x: event.clientX, y: event.clientY };
			transformStart = { x: transform.x, y: transform.y };
		}
	}

	function handleMouseMove(event) {
		if (isPanning) {
			const dx = event.clientX - panStart.x;
			const dy = event.clientY - panStart.y;

			transform = {
				...transform,
				x: transformStart.x + dx,
				y: transformStart.y + dy
			};
		}
	}

	function handleMouseUp() {
		isPanning = false;
	}

	// Expose zoom controls
	export function zoomIn() {
		const newScale = Math.min(MAX_ZOOM, transform.scale * 1.2);
		transform = { ...transform, scale: newScale };
		if (onZoomChange) onZoomChange(newScale);
	}

	export function zoomOut() {
		const newScale = Math.max(MIN_ZOOM, transform.scale / 1.2);
		transform = { ...transform, scale: newScale };
		if (onZoomChange) onZoomChange(newScale);
	}

	export function resetZoom() {
		transform = { x: 0, y: 0, scale: 1 };
		if (onZoomChange) onZoomChange(1);
	}

	export function getTransform() {
		return transform;
	}

	export function setTransform(newTransform) {
		transform = newTransform;
		if (onZoomChange) onZoomChange(newTransform.scale);
	}
</script>

<svelte:window
	onmousemove={handleMouseMove}
	onmouseup={handleMouseUp}
/>

<div
	bind:this={canvasContainer}
	class="infinite-canvas-container"
	class:panning={isPanning}
	role="button"
	aria-label="Infinite canvas viewport"
	tabindex="0"
	onwheel={handleWheel}
	onmousedown={handleMouseDown}
>
	<div
		class="infinite-canvas-content"
		style="transform: translate({transform.x}px, {transform.y}px) scale({transform.scale})"
	>
		{@render children()}
	</div>
</div>

<style>
	.infinite-canvas-container {
		width: 100%;
		height: 100%;
		overflow: hidden;
		position: relative;
	}

	.infinite-canvas-container.panning {
		cursor: grabbing;
	}

	.infinite-canvas-content {
		transform-origin: 0 0;
		position: absolute;
		top: 0;
		left: 0;
		width: 100%;
		height: 100%;
	}
</style>
