<script>
  import { onMount } from "svelte";
  import { goto } from "$app/navigation";
  import { invoke } from "@tauri-apps/api/core";
  import SetupWizard from "$lib/components/SetupWizard.svelte";

  let setupComplete = null; // null = checking, true = done, false = not done

  onMount(async () => {
    try {
      const isDone = await invoke('setup__check');

      if (isDone) {
        setupComplete = true;
        goto("/graph");
      } else {
        setupComplete = false;
      }
    } catch (error) {
      console.log('Setup check failed, showing wizard:', error);
      setupComplete = false;
    }
  });

  function handleSetupComplete(event) {
    console.log('Setup complete:', event.detail);
    setupComplete = true;
    goto("/graph");
  }
</script>

<main class="container">
  {#if setupComplete === null}
    <p class="redirecting">Checking setup...</p>
  {:else if setupComplete === false}
    <SetupWizard on:complete={handleSetupComplete} />
  {:else}
    <p class="redirecting">Redirecting to graph view...</p>
  {/if}
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
