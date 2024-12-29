<script lang="ts">
  import {listen} from "@tauri-apps/api/event";

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

<container>
  <spinner/>
  {#if importProgress}
    {#if importProgress.total_documents > 1}
      <p>Processing document {importProgress.current_document} of {importProgress.total_documents}</p>
    {/if}
    {#if importProgress.total_pages > 1}
      <p>Page {importProgress.current_page} of {importProgress.total_pages}</p>
    {/if}
  {/if}
</container>

<style>
    container {
        display: flex;
        flex-direction: column;
        align-items: center;
        justify-content: center;
        height: 100vh;
    }
    spinner {
        width: 48px;
        height: 48px;
        border: 5px solid #FFF;
        border-bottom-color: transparent;
        border-radius: 50%;
        display: inline-block;
        box-sizing: border-box;
        animation: rotation 1s linear infinite;
        margin-bottom: 2em;
    }

    @keyframes rotation {
        0% {
            transform: rotate(0deg);
        }
        100% {
            transform: rotate(360deg);
        }
    }
</style>
