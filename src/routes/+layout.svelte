<script>
  import '$lib/fonts.css';
  import '$lib/colors.css';
  import { initializeLogging } from '$lib/logging.js';
  import { onMount } from 'svelte';

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

  // Initialize logging when app mounts
  onMount(() => {
    initializeLogging();
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
