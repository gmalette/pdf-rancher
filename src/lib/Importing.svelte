<script lang="ts">
  import {listen} from "@tauri-apps/api/event";
  import Progress from "./Progress.svelte";

  type ImportProgress = {
    current_document: number,
    total_documents: number,
    current_page: number,
    total_pages: number,
  }

  let importProgress: ImportProgress | null = $state(null);

  listen("rancher://did-open-file-page", (e) => {
    importProgress = e.payload as ImportProgress;
  })
</script>

<Progress>
  {#if importProgress}
    {#if importProgress.total_documents > 1}
      <p>Processing document {importProgress.current_document} of {importProgress.total_documents}</p>
    {/if}
    {#if importProgress.total_pages > 1}
      <p>Page {importProgress.current_page} of {importProgress.total_pages}</p>
    {/if}
  {/if}
</Progress>
