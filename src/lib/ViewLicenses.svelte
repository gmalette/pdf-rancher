<script lang="ts">
  import {invoke} from "@tauri-apps/api/core";

  type License = {
    component: String,
    origin: String,
    license: String,
    copyright: String,
  };

  let licenses: null | License[] = $state(null);
  let { close }: { close: () => void } = $props();

  $effect(() => {
    invoke("licenses_command").then((response: any) => {
      licenses = response;
    })
  });

  function handleKeyPress(e: KeyboardEvent) {
    if (e.key === "Escape") {
      close()
    }
  }
</script>

<svelte:window onkeypress={handleKeyPress}/>

{#if licenses === null}
  <p>Loading...</p>
{:else}
  <button onclick={close}>Close</button>

  <h1>Licenses</h1>
  {#each licenses as license}
    <div>
      <h2>{license.component}</h2>
      <p>Origin: {license.origin}</p>
      <p>License: {license.license}</p>
      <p>Copyright: {license.copyright}</p>
    </div>
  {/each}
{/if}
