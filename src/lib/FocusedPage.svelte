<script lang="ts">
  import type {Ordering, Page} from "./project";
  import {previewToDataUrl} from "./project.js";
  import Preview from "./Preview.svelte";
  import {invoke} from "@tauri-apps/api/core";
  import Progress from "./Progress.svelte";

  let { ordering, closeFocus }: { ordering: Ordering, closeFocus: (a: number) => void } = $props();
  let rotation = ordering.rotation;
  let newRotation: number = $state(rotation);
  let page: Page | null = $state(null);

  $effect(() => {
    invoke("preview_command", { ordering: {...ordering, rotation: "0" } }).then((response: any) => {
      page = response;
    });
  });

  function rotateCW() {
    newRotation = (newRotation + 90) % 360;
  }

  function rotateCCW() {
    newRotation = (newRotation + 270) % 360;
  }

  function closeAndSave(_: MouseEvent) {
    closeFocus(newRotation);
  }

  function handleKeyPress(e: KeyboardEvent) {
    if (e.key === "Escape") {
      closeFocus(newRotation);
    }
  }
</script>

<svelte:window onkeypress={handleKeyPress}/>

{#if page !== null}
  <div>
    <tools>
      <button onclick={closeAndSave}>Close</button>
      <button onclick={rotateCW} aria-label="Rotate clockwise">
        <i class="fas fa-redo"></i>
      </button>
      <button onclick={rotateCCW} aria-label="Rotate counter-clockwise">
        <i class="fas fa-undo"></i>
      </button>
    </tools>
    <Preview fullSize={true} jpg={page.preview_jpg} rotation={newRotation} pageNum={1} />
  </div>
{:else}
  <Progress />
{/if}

<style>
  tools {
      display: block;
      padding-bottom: 1rem;
  }
</style>
