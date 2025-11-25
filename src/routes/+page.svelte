<script>
  import { onMount } from "svelte";
  import { goto } from "$app/navigation";

  onMount(() => {
    // Redirect to graph view as default
    goto("/graph");
  });
</script>

<main class="container">
  <p class="redirecting">Redirecting to graph view...</p>
</main>

<style>
  .container {
    display: flex;
    align-items: center;
    justify-content: center;
    height: 100vh;
    background: #1a1a1a;
  }

  .redirecting {
    color: #ff8c42;
    font-size: 1.2em;
  }
</style>
