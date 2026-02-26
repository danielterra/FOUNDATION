<script>
  import ChatWindow from '$lib/components/ChatWindow.svelte';

  // Chat is always open in the new design
  let isChatOpen = true;
</script>

<div class="main-layout">
  <!-- Left side: Canvas placeholder (for Phase 2) -->
  <div class="canvas-area">
    <div class="placeholder">
      <span class="material-symbols-outlined">dashboard</span>
      <h2>Canvas Area</h2>
      <p>Widget space reserved for Phase 2</p>
      <p class="hint">The AI will be able to add and manage widgets here</p>
    </div>
  </div>

  <!-- Right side: Fixed Chat -->
  <div class="chat-area">
    <ChatWindow bind:isOpen={isChatOpen} />
  </div>
</div>

<style>
  .main-layout {
    display: flex;
    width: 100vw;
    height: 100vh;
    overflow: hidden;
  }

  .canvas-area {
    flex: 1;
    display: flex;
    align-items: center;
    justify-content: center;
    background: transparent;
    position: relative;
    z-index: 1;
  }

  .placeholder {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 16px;
    padding: 40px;
    text-align: center;
    color: var(--color-neutral);
    background: color-mix(in srgb, var(--color-white) 5%, transparent);
    border: 2px dashed color-mix(in srgb, var(--color-white) 20%, transparent);
    border-radius: 16px;
    max-width: 500px;
  }

  .placeholder .material-symbols-outlined {
    font-size: 64px;
    opacity: 0.5;
    color: var(--color-interactive);
  }

  .placeholder h2 {
    margin: 0;
    font-size: 28px;
    font-weight: 600;
    color: var(--color-neutral-active);
  }

  .placeholder p {
    margin: 0;
    font-size: 16px;
    color: var(--color-neutral);
  }

  .placeholder .hint {
    font-size: 14px;
    color: var(--color-neutral-disabled);
    font-style: italic;
  }

  .chat-area {
    width: 30%;
    min-width: 500px;
    height: 100vh;
    background: color-mix(in srgb, var(--color-black) 40%, transparent);
    border-left: 1px solid color-mix(in srgb, var(--color-white) 15%, transparent);
    position: relative;
    z-index: 100;
    display: flex;
    flex-direction: column;
  }
</style>
