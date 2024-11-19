<script lang="ts">
  import {attachConsole} from "@tauri-apps/plugin-log";
  import {invoke} from '@tauri-apps/api/core'
  import '@fortawesome/fontawesome-free/css/all.min.css'
  import {listen} from "@tauri-apps/api/event";

  type Page = {
    preview_jpg: string
  }

  type SourceFile = {
    pages: Page[]
    path: string,
  }

  type Project = {
    source_files: SourceFile[]
  }

  let count: number = $state(0)
  let project: Project = $state({ source_files: [] })

  const loadProject = () => {
    invoke("load_project").then((response: any) => {
      project = response.project as Project;
    });
  }

  $effect(() => {
    loadProject()
  })

  const openFiles = () => {
    count += 1
    invoke("open_files").then((response: any) => {
      project = response.project as Project;
    });

  }

  const previewToDataUrl = (preview_jpg: string) => {
    return "data:image/jpg;base64," + preview_jpg
  }

  const baseName = (path: string) => {
    return path.split('/').pop()
  }

  listen("files-did-open", () => {
    loadProject()
  })

  attachConsole()
</script>

<main>
  <div id="files">
    {#each project.source_files as source_file}
      <p>{baseName(source_file.path)}</p>
      <div class="file">
        {#each source_file.pages as page, i}
          <div class="page">
            <img src={previewToDataUrl(page.preview_jpg)} alt="Page preview for page number {i + 1}"/>
          </div>
        {/each}
      </div>
    {/each}
  </div>

  <button onclick={openFiles}>
    <i class="fa-regular fa-file-circle-plus"></i>
  </button>
</main>

<style>
    main {
        flex-grow: 100;
    }
    .file {
        display: flex;
        overflow-x: scroll;
        height: 200px;
    }
    .page {
        padding: 0 10px;
        height: 100%;
        width: 100%;

        img {
            height: 100%;
            max-width: 100%;
            object-fit: contain;
        }

    }
</style>
