<script>
  import { onMount } from 'svelte';
  import { goto } from "$app/navigation";
  import { invoke } from "@tauri-apps/api/core";
  import SetupWizard from "$lib/components/SetupWizard.svelte";
  import ImportProgress from "$lib/components/ImportProgress.svelte";

  let setupComplete = null; // null = checking, true = done, false = not done
  let importing = null; // null = checking, true = importing, false = already imported

  onMount(async () => {
    console.log('+page: Checking if database is already initialized...');
    checkDatabaseStatus();
  });

  async function checkDatabaseStatus() {
    try {
      // Check if database is initialized by checking if setup is complete
      // This will fail if database doesn't exist or isn't initialized
      const isSetupDone = await invoke('setup__check');
      console.log('+page: Setup check result:', isSetupDone);

      // Database exists and is initialized, skip import screen
      importing = false;

      if (isSetupDone) {
        setupComplete = true;
        goto("/canvas");
      } else {
        setupComplete = false;
      }
    } catch (error) {
      const errorMsg = String(error);
      console.log('+page: Database check failed:', errorMsg);

      // If error is about state not managed, database is still initializing
      if (errorMsg.includes('state not managed') || errorMsg.includes('conn')) {
        console.log('+page: Database still initializing...');

        // Show import screen if not already showing
        if (importing === null) {
          importing = true;
        }

        setTimeout(() => checkDatabaseStatus(), 500);
      } else {
        // Other errors mean database doesn't exist, show import screen
        console.log('+page: Database not initialized, showing import screen');
        importing = true;
      }
    }
  }

  async function handleImportComplete() {
    console.log('+page: Import completed, checking setup status...');
    importing = false;

    // Now check if setup is complete
    try {
      const isDone = await invoke('setup__check');
      console.log('+page: Setup check result:', isDone);

      if (isDone) {
        setupComplete = true;
        goto("/canvas");
      } else {
        setupComplete = false;
      }
    } catch (error) {
      const errorMsg = String(error);
      console.log('+page: Setup check failed:', errorMsg);

      // If error is about state not managed, database is still initializing
      if (errorMsg.includes('state not managed') || errorMsg.includes('conn')) {
        console.log('+page: Database still initializing, retrying...');
        setTimeout(() => checkSetup(), 500);
      } else {
        // Other errors, show setup wizard
        console.log('+page: Unexpected error, showing setup wizard');
        setupComplete = false;
      }
    }
  }

  function handleSetupComplete(event) {
    console.log('+page: Setup wizard completed:', event.detail);
    setupComplete = true;
    goto("/canvas");
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
    <p class="redirecting">Redirecting to canvas...</p>
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
