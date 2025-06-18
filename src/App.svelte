<script lang="ts">
  import {attachConsole, info} from "@tauri-apps/plugin-log";
  import {invoke} from '@tauri-apps/api/core'
  import {listen} from "@tauri-apps/api/event";
  import {dndzone} from 'svelte-dnd-action';
  import Banners from "./lib/Banners.svelte";
  import FocusedPage from "./lib/FocusedPage.svelte";
  import {type Ordering, type Project, type SourceFile} from "./lib/project";
  import Preview from "./lib/Preview.svelte";
  import {
    DRAGGING_OVER,
    DraggingOverState,
    EXPORTING,
    ExportingState,
    FOCUSED,
    type Focused,
    FocusedState,
    IMPORTING,
    ImportingState,
    LICENSE, LicenseState,
    LIST,
    ListState,
    type UiState
  } from "./lib/ui_state";
  import Importing from "./lib/Importing.svelte";
  import Exporting from "./lib/Exporting.svelte";
  import ViewLicenses from "./lib/ViewLicenses.svelte";
  import Progress from "./lib/Progress.svelte";

  let project: Project = $state({ source_files: [], ordering: [] })
  let uiState: UiState = $state(ListState())

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

  function beginExport() {
    // select only enabled pages
    const ordering = project.ordering.filter((ordering) => ordering.enabled).map((ordering) => {
      return {...ordering, rotation: ordering.rotation.toString()}
    })
    invoke("export_command", { ordering })
  }

  $effect(() => {
    loadProject()
  })

  listen("rancher://will-open-files", () => {
    uiState = ImportingState()
  })

  listen("rancher://did-open-files", () => {
    loadProject().then(() => {
      uiState = ListState()
    })
  })

  listen("rancher://export-requested", () => {
    beginExport()
  })

  listen("rancher://licenses-requested", () => {
    info("licences-requested")
    uiState = LicenseState()
  })

  listen("rancher://will-export", () => {
    uiState = ExportingState()
  })

  listen("rancher://did-export", () => {
    uiState = ListState()
  })

  listen("rancher://did-not-export", () => {
    uiState = ListState()
  })

  listen("rancher://did-clear-project", () => {
    loadProject()
  })

  listen("tauri://drag-over", () => {
    info("drag-over")
    uiState = DraggingOverState()
  })

  listen("tauri://drag-leave", () => {
    uiState = ListState()
  })

  listen("tauri://drag-drop", () => {
    uiState = ListState()
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
     uiState = FocusedState(pageNum)
  }

  function setRotation(pageNum: number, newRotation: number) {
    const ordering = project.ordering[pageNum]
    const newOrdering = {
      ...ordering,
      rotation: newRotation,
    }

    project = {
      ...project,
      ordering: [
        ...project.ordering.slice(0, pageNum),
        newOrdering,
        ...project.ordering.slice(pageNum + 1),
      ],
    }
  }

  function closeFocus(focusedState: Focused, newRotation: number) {
    const focused = focusedState.ordering

    setRotation(focused, newRotation)

    uiState = ListState()
  }

  function handleKeyPress(e: KeyboardEvent) {
    if (uiState.type !== LIST) {
      return
    }

    // Q rotates counterclockwise, E rotates clockwise
    if ((e.key === "q" || e.key === "e") && !(e.metaKey || e.ctrlKey || e.altKey || e.shiftKey)) {
      const page = document.querySelector("page:hover");
      if (!page) {
        return;
      }
      const index = parseInt(page.getAttribute("index")!);
      const ordering = project.ordering[index];

      let newRotation;
      if (e.key === "q") {
        newRotation = (ordering.rotation + 270) % 360;
      } else {
        newRotation = (ordering.rotation + 90) % 360;
      }

      return setRotation(index, newRotation);
    }
  }

  attachConsole();
</script>

<svelte:window onkeypress={handleKeyPress}/>

<Banners/>

<project>
  {#if uiState.type === IMPORTING}
    <Importing/>
  {:else if uiState.type === LICENSE }
    <ViewLicenses close={() => uiState = ListState() }/>
  {:else if project.source_files.length === 0 || uiState.type === DRAGGING_OVER}
    <dropzone class:active={uiState.type === DRAGGING_OVER}>
      <i class="fa-solid fa-file-circle-plus"></i>
    </dropzone>
  {:else if uiState.type === FOCUSED}
    <FocusedPage ordering={project.ordering[uiState.ordering]} page={page(project.ordering[uiState.ordering])} closeFocus={(e) => closeFocus(uiState, e)}/>
  {:else if uiState.type === EXPORTING}
    <Exporting/>
  {:else}
    <previews use:dndzone={{items: project.ordering, flipDurationMs: 100}} onconsider={handleDnd} onfinalize={handleDnd}>
      {#each project.ordering as ordering, pageNum (ordering.id)}
        <page
            oncontextmenu={(e: MouseEvent) => onContextMenu(e, pageNum)}
            onclick={(_: MouseEvent) => onPageClick(pageNum)}
            class:disabled={!ordering.enabled}
            index={pageNum}>

          <Preview jpg={page(ordering).preview_jpg} rotation={ordering.rotation} pageNum={pageNum + 1}/>

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
