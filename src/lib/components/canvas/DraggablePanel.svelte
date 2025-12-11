<script>
	let {
		position = { x: 0, y: 0 },
		onPositionChange = null,
		canvasTransform = { x: 0, y: 0, scale: 1 },
		children
	} = $props();

	let isDragging = $state(false);
	let dragStart = $state({ x: 0, y: 0 });
	let positionStart = $state({ x: 0, y: 0 });

	function handleDragStart(event) {
		// Only drag if clicking on the drag handle area
		if (!event.target.closest('.panel-drag-handle')) {
			return;
		}

		// Don't drag if clicking on buttons or interactive elements
		if (event.target.closest('button, input, textarea, select, a')) {
			return;
		}

		isDragging = true;
		dragStart = { x: event.clientX, y: event.clientY };
		positionStart = { ...position };
		event.preventDefault();
		event.stopPropagation();
	}

	function handleDragMove(event) {
		if (!isDragging) return;

		// Calculate movement in screen space
		const dx = event.clientX - dragStart.x;
		const dy = event.clientY - dragStart.y;

		// Convert to canvas space (accounting for zoom)
		const canvasDx = dx / canvasTransform.scale;
		const canvasDy = dy / canvasTransform.scale;

		const newPosition = {
			x: positionStart.x + canvasDx,
			y: positionStart.y + canvasDy
		};

		if (onPositionChange) {
			onPositionChange(newPosition);
		}
	}

	function handleDragEnd() {
		isDragging = false;
	}
</script>

<svelte:window
	onmousemove={handleDragMove}
	onmouseup={handleDragEnd}
/>

<div
	class="draggable-panel"
	class:dragging={isDragging}
	onwheel={(e) => e.stopPropagation()}
>
	<div class="drag-detector" onmousedown={handleDragStart}>
		{@render children()}
	</div>
</div>

<style>
	.draggable-panel {
		position: relative;
	}

	.drag-detector {
		width: 100%;
		height: 100%;
	}

	.draggable-panel.dragging {
		user-select: none;
	}

	.draggable-panel.dragging * {
		pointer-events: none;
	}

	.draggable-panel.dragging .panel-drag-handle {
		pointer-events: auto;
		cursor: grabbing !important;
	}
</style>
