<script lang="ts">
  import type {Page} from "./project";
  import {previewToDataUrl} from "./project.js";
  import Preview from "./Preview.svelte";

  let { rotation, page, closeFocus }: { rotation: number, page: Page, closeFocus: (a: number) => void } = $props();
  let newRotation: number = $state(rotation);

  function rotateCW() {
    newRotation = (newRotation + 90) % 360;
  }

  function rotateCCW() {
    newRotation = (newRotation + 270) % 360;
  }

  function closeAndSave(_: MouseEvent) {
    closeFocus(newRotation);
  }
</script>

<div>
  <tools>
    <button onclick={closeAndSave}>Close</button>
    <button onclick={rotateCW}>
      <i class="fas fa-redo"></i>
    </button>
    <button onclick={rotateCCW}>
      <i class="fas fa-undo"></i>
    </button>
  </tools>
  <Preview fullSize={true} jpg={page.preview_jpg} rotation={newRotation} pageNum={1} />
</div>

<style>
  tools {
      display: block;
      padding-bottom: 1rem;
  }
</style>
