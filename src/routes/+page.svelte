<script>
  import { onMount } from "svelte";
  import { goto } from "$app/navigation";
  import { invoke } from "@tauri-apps/api/core";
  import SetupWizard from "$lib/components/SetupWizard.svelte";

  let setupComplete = null; // null = checking, true = done, false = not done

  onMount(async () => {
    // Check if setup exists by trying to call setup__init with a test
    // We'll check if foundation:ThisFoundationInstance exists
    try {
      // Try calling setup__init - if already_setup is true, redirect to graph
      const result = await invoke('setup__init', {
        userName: "_check_setup_",
        email: null
      });

      if (result.alreadySetup) {
        // Setup already exists, go to graph
        setupComplete = true;
        goto("/graph");
      } else {
        // Oops, we just created a setup with name "_check_setup_"
        // This shouldn't happen in production, but show setup wizard anyway
        setupComplete = false;
      }
    } catch (error) {
      // Error means setup doesn't exist, show wizard
      console.log('Setup not found, showing wizard');
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
