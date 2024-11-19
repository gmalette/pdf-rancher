<script lang="ts">
  import {attachConsole} from "@tauri-apps/plugin-log";
  import {invoke} from '@tauri-apps/api/core'
  import '@fortawesome/fontawesome-free/css/all.min.css'
  import {listen} from "@tauri-apps/api/event";
  import Files from "./lib/Files.svelte";

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
  let isDraggingFilesOver: boolean = $state(false)

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

  listen("tauri://drag-over", () => {
    isDraggingFilesOver = true
  })

  listen("tauri://drag-leave", () => {
    isDraggingFilesOver = false
  })

  attachConsole()
</script>

<project>
  {#if project.source_files.length === 0}
    <dropzone class="{isDraggingFilesOver ? 'active' : ''}">
      <i class="fa-regular fa-file-circle-plus"></i>
    </dropzone>
  {:else}
    {#each project.source_files as source_file}
      <file>
        <p>{baseName(source_file.path)}</p>
        <previews>
          {#each source_file.pages as page, i}
            <page>
              <img src={previewToDataUrl(page.preview_jpg)} alt="Page preview for page number {i + 1}"/>
            </page>
          {/each}
        </previews>
      </file>
    {/each}
  {/if}
</project>

<style>
    dropzone {
        display: flex;
        align-items: center;
        justify-content: center;
        height: 100%;
        text-align: center;
        padding: 20px;
        border: 2px dashed var(--dropzone-grey);
        border-radius: 10px;

        i {
            font-size: 50px;
            color: var(--dropzone-grey);
        }

        &.active {
            border-color: var(--active-color);
            box-shadow: inset 0 0 100px rgba(0, 0, 0, 0.2);

            i {
                color: var(--active-color);
            }
        }
    }

    file {
        display: block;
        margin-bottom: 2rem;

        &:nth-child(odd) {
            background-color: var(--file-background-color);
        }
    }

    previews {
        display: block;
        overflow-x: auto;
        overflow-y: hidden;
        white-space: nowrap;
    }

    page {
        display: inline-block;

        img {
            height: var(--file-height);
            max-width: 100%;
            box-shadow: 2px 2px 4px rgba(0, 0, 0, 0.2);
        }
    }

    page:not(:last-child) {
        margin-right: var(--page-margin-right);
    }
    page:first-child {
        padding-left: 1px;
    }
</style>
