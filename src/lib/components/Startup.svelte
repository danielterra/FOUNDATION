<script>
  import { onMount } from 'svelte';
  import { goto } from "$app/navigation";
  import { invoke } from "@tauri-apps/api/core";
  import SetupWizard from "$lib/components/SetupWizard.svelte";
  import ImportProgress from "$lib/components/ImportProgress.svelte";

  let setupComplete = $state(null); // null = checking, true = done, false = not done
  let importing = $state(null); // null = checking, true = importing, false = already imported

  onMount(async () => {
    checkDatabaseStatus();
  });

  async function checkDatabaseStatus() {
    try {
      const isSetupDone = await invoke('setup__check');
      importing = false;
      if (isSetupDone) {
        setupComplete = true;
        goto("/home");
      } else {
        setupComplete = false;
      }
    } catch (error) {
      const errorMsg = String(error);

      if (errorMsg.includes('state not managed')) {
        if (importing === null) importing = true;
        const { listen } = await import('@tauri-apps/api/event');
        const unlisten = await listen('import-complete', async () => {
          unlisten();
          await checkDatabaseStatus();
        });
      } else {
        importing = true;
      }
    }
  }

  async function handleImportComplete() {
    importing = false;

    try {
      const isDone = await invoke('setup__check');

      if (isDone) {
        setupComplete = true;
        goto("/home");
      } else {
        setupComplete = false;
      }
    } catch (error) {
      const errorMsg = String(error);

      if (errorMsg.includes('state not managed') || errorMsg.includes('conn')) {
        setTimeout(() => checkDatabaseStatus(), 500);
      } else {
        setupComplete = false;
      }
    }
  }

  function handleSetupComplete(event) {
    setupComplete = true;
    goto("/home");
  }
</script>

<main class="container">
  {#if importing === null}
    <p class="redirecting">Checking database...</p>
  {:else if importing === true}
    <ImportProgress onComplete={handleImportComplete} />
  {:else if setupComplete === null}
    <p class="redirecting">Checking setup...</p>
  {:else if setupComplete === false}
    <SetupWizard onComplete={handleSetupComplete} />
  {:else}
    <p class="redirecting">Redirecting...</p>
  {/if}
</main>

<style>
  .container {
    display: flex;
    align-items: center;
    justify-content: center;
    height: 100vh;
    background: var(--color-black);
  }

  .redirecting {
    color: var(--color-neutral);
    font-size: 1.2em;
  }
</style>
