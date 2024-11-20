<script lang="ts">
  import {attachConsole, info} from "@tauri-apps/plugin-log";
  import {invoke} from '@tauri-apps/api/core'
  import '@fortawesome/fontawesome-free/css/all.min.css'
  import {listen} from "@tauri-apps/api/event";
  import { dndzone } from 'svelte-dnd-action';

  type Page = {
    preview_jpg: string
  }

  type SourceFile = {
    pages: Page[]
    path: string,
  }

  type Ordering = {
    id: number,
    source_file_index: number,
    page_index: number,
  }

  type ProjectResponse = {
    source_files: SourceFile[],
  }

  type Project = {
    source_files: SourceFile[],
    ordering: Ordering[],
  }

  let project: Project = $state({ source_files: [], ordering: [] })
  let isDraggingFilesOver: boolean = $state(false)

  const updateProject = (newProject: ProjectResponse) => {
    let newOrdering = []
    let index = 0;
    for (let i = 0; i < newProject.source_files.length; i++) {
      let source_file = newProject.source_files[i];
      for (let j = 0; j < source_file.pages.length; j++) {
        let oldOrdering = project.ordering[index];
        if (oldOrdering) {
          newOrdering.push(oldOrdering)
        } else {
          newOrdering.push({id: index, source_file_index: i, page_index: j})
        }
        index += 1
      }
    }

    project = {
      source_files: newProject.source_files,
      ordering: newOrdering,
    }
  }

  const loadProject = () => {
    invoke("load_project").then((response: any) => {
      updateProject(response.project as ProjectResponse);
    });
  }

  $effect(() => {
    loadProject()
  })

  const openFiles = () => {
    invoke("open_files").then((response: any) => {
      updateProject(response.project as ProjectResponse);
    })
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

  listen("tauri://drag-drop", () => {
    isDraggingFilesOver = false
  })

  function handleDnd(e: any) {
    project = {
      ...project,
      ordering: e.detail.items,
    }
  }

  function page(ordering: Ordering) {
    return project.source_files[ordering.source_file_index].pages[ordering.page_index]
  }

  attachConsole();
</script>

<project>
  {#if project.source_files.length === 0}
    <dropzone class="{isDraggingFilesOver ? 'active' : ''}">
      <i class="fa-regular fa-file-circle-plus"></i>
    </dropzone>
  {:else}
    <previews use:dndzone={{items: project.ordering, flipDurationMs: 100}} onconsider={handleDnd} onfinalize={handleDnd}>
      {#each project.ordering as ordering, pageNum (ordering.id)}
        <page>
          <img src={previewToDataUrl(page(ordering).preview_jpg)} alt="Page preview for page number {pageNum + 1}"/>
          <p>{pageNum + 1}</p>
        </page>
      {/each}
    </previews>
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
        /*overflow-x: auto;*/
        /*overflow-y: hidden;*/
        /*white-space: nowrap;*/
    }

    page {
        display: inline-block;
        text-align: center;
        padding-bottom: 1rem;

        p {
            padding: 0;
            margin: 0;
        }

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
