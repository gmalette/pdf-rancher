<script lang="ts">
  import {attachConsole, info} from "@tauri-apps/plugin-log";
  import {invoke} from '@tauri-apps/api/core'
  import {listen} from "@tauri-apps/api/event";
  import { dndzone } from 'svelte-dnd-action';
  import Banners from "./lib/Banners.svelte";
  import FocusedPage from "./lib/FocusedPage.svelte";
  import {type Ordering, type Project, type SourceFile} from "./lib/project";
  import Preview from "./lib/Preview.svelte";
  import OpeningFiles from "./lib/OpeningFiles.svelte";

  let project: Project = $state({ source_files: [], ordering: [] })
  let isDraggingFilesOver: boolean = $state(false)
  let isOpeningFiles: boolean = $state(false)
  let focused: number | null = $state(null)

  type ProjectResponse = {
    source_files: SourceFile[],
  }

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
          newOrdering.push({ id: index, source_file_index: i, page_index: j, enabled: true, rotation: 0 })
        }
        index += 1
      }
    }

    project = {
      source_files: newProject.source_files,
      ordering: newOrdering,
    }
  }

  async function loadProject() {
    invoke("load_project_command").then((response: any) => {
      updateProject(response.project as ProjectResponse);
    });
  }

  $effect(() => {
    loadProject()
  })

  listen("rancher://will-open-files", () => {
    isOpeningFiles = true
  })

  listen("rancher://did-open-files", () => {
    loadProject().then(() => {
      isOpeningFiles = false
    })
  })

  listen("rancher://export-requested", () => {
    // select only enabled pages
    const ordering = project.ordering.filter((ordering) => ordering.enabled).map((ordering) => {
      return {...ordering, rotation: ordering.rotation.toString()}
    })
    invoke("export_command", { ordering })
  })

  listen("tauri://drag-over", () => {
    info("drag-over")
    isDraggingFilesOver = true
  })

  listen("tauri://drag-leave", () => {
    info("drag-leave")
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

  function onContextMenu(e: MouseEvent, targetIndex: number) {
    e.preventDefault()

    const oldOrdering = project.ordering[targetIndex]

    const newOrdering = {
      ...oldOrdering,
      enabled: !oldOrdering.enabled,
    }

    const ordering = [
      ...project.ordering.slice(0, targetIndex),
      newOrdering,
      ...project.ordering.slice(targetIndex + 1),
    ]

    // Disable the ordering
    project = {
      ...project,
      ordering,
    }
  }

  function onPageClick(pageNum: number) {
    focused = pageNum
  }

  function closeFocus(newRotation: number) {
    const oldOrdering = project.ordering[focused!];
    const newOrdering = {
      ...oldOrdering,
      rotation: newRotation,
    }
    project = {
      ...project,
      ordering: [
        ...project.ordering.slice(0, focused!),
        newOrdering,
        ...project.ordering.slice(focused! + 1),
      ],
    }
    focused = null
  }

  attachConsole();
</script>

<Banners/>

<project>
  {#if isOpeningFiles}
    <OpeningFiles/>
  {:else if project.source_files.length === 0 || isDraggingFilesOver}
    <dropzone class:active={isDraggingFilesOver}>
      <i class="fa-solid fa-file-circle-plus"></i>
    </dropzone>
  {:else if focused !== null}
    <FocusedPage rotation={project.ordering[focused].rotation} page={page(project.ordering[focused])} {closeFocus}/>
  {:else}
    <previews use:dndzone={{items: project.ordering, flipDurationMs: 100}} onconsider={handleDnd} onfinalize={handleDnd}>
      {#each project.ordering as ordering, pageNum (ordering.id)}
        <page
            oncontextmenu={(e: MouseEvent) => onContextMenu(e, pageNum)}
            onclick={(_: MouseEvent) => onPageClick(pageNum)}
            class:disabled={!ordering.enabled}>

          <Preview jpg="{page(ordering).preview_jpg}" rotation={ordering.rotation} pageNum={pageNum + 1}/>

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
    }

    page {
        display: inline-block;
        text-align: center;
        padding-bottom: 1rem;

        &.disabled {
            opacity: 0.1;
        }

        p {
            padding: 0;
            margin: 0;
        }
    }

    page:not(:last-child) {
        margin-right: var(--page-margin-right);
    }

    page:first-child {
        padding-left: 1px;
    }

    page:not(:last-child) {
        margin-right: var(--page-margin-right);
    }
    page:first-child {
        padding-left: 1px;
    }
</style>
