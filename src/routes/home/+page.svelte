<script>
  import ChatWindow from '$lib/components/ChatWindow.svelte';
  import WidgetManager from '$lib/components/widgets/WidgetManager.svelte';

  // Chat is always open in the new design
  let isChatOpen = true;
</script>

<div class="main-layout">
  <!-- Left side: Canvas with widgets -->
  <div class="canvas-area">
    <WidgetManager />
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
