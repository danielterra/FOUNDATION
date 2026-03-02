<script>
  import '$lib/fonts.css';
  import '$lib/colors.css';
  import { initializeLogging } from '$lib/logging.js';
  import ChatWindow from '$lib/components/ChatWindow.svelte';
  import { onMount } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';

  // Available background videos
  const backgroundVideos = [
    '/background-space.mp4',
    '/background-code.mp4',
    '/background-dust.mp4',
    '/background-edges.mp4',
    '/background-particles.mp4'
  ];

  // Select random video
  const selectedVideo = backgroundVideos[Math.floor(Math.random() * backgroundVideos.length)];

  // Initialize logging and database when app mounts
  onMount(async () => {
    initializeLogging();

    // Initialize database - this will request folder permissions if needed
    try {
      await invoke('initialize_app');
    } catch (err) {
      console.error('[App] Failed to initialize database:', err);
      // TODO: Show error UI with "Retry" button
      alert('Failed to initialize database. Please check permissions and try again.');
    }
  });
</script>

<!-- Background Video (global) -->
<video
  autoplay
  loop
  muted
  playsinline
  class="background-video"
>
  <source src={selectedVideo} type="video/mp4" />
</video>

<slot />

<style>
  :global(body) {
    margin: 0;
    padding: 0;
    overflow: hidden;
  }

  .background-video {
    position: fixed;
    top: 0;
    left: 0;
    width: 100vw;
    height: 100vh;
    object-fit: cover;
    z-index: 0;
    opacity: 0.2;
    pointer-events: none;
  }
</style>
