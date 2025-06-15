mod licenses;
mod project;

use crate::licenses::License;
use crate::project::{Page, Project, Selector};
use log::error;
use project::SourceFile;
use serde::Serialize;
use std::path::PathBuf;
use std::sync::{mpsc, Mutex};
use tauri::menu::*;
use tauri::menu::{MenuBuilder, SubmenuBuilder};
use tauri::Manager;
use tauri::{AppHandle, Emitter};
use tauri_plugin_dialog::{DialogExt, FilePath, MessageDialogButtons};

/// TO RELEASE
/// cargo tauri build --runner cargo-xwin --target x86_64-pc-windows-msvc && cargo tauri build --runner cargo-xwin --target aarch64-pc-windows-msvc && cargo tauri build
/// version=$(cargo metadata --format-version=1 --no-deps | jq -r '.packages[0].version') && mkdir $version && mv target/**/*$version* $version

#[derive(Debug, Clone, Serialize)]
struct AppState {
    project: Project,
}

impl AppState {
    fn new() -> Self {
        Self {
            project: Project::new(),
        }
    }

    fn add_source_files(&mut self, new_files: Vec<SourceFile>) {
        self.project.add_source_files(new_files);
    }
}

#[derive(Debug, Clone, Serialize)]
struct ImportProgress {
    current_document: usize,
    total_documents: usize,
    current_page: usize,
    total_pages: usize,
}

async fn add_files(app: AppHandle, paths: Vec<PathBuf>) -> Result<(), String> {
    if paths.is_empty() {
        return Ok(());
    };

    let _ = app.emit("rancher://will-open-files", ());

    let app_state = app.state::<Mutex<AppState>>();

    let mut new_files = Vec::new();
    let total_documents = paths.len();

    for (index, path) in paths.into_iter().enumerate() {
        let (sender, receiver) = mpsc::channel();

        let import_task = tauri::async_runtime::spawn_blocking(move || {
            SourceFile::open(&path, Some(sender.clone()))
        });

        let receiver_app = app.clone();

        let update_progress_task = tauri::async_runtime::spawn(async move {
            for (current_page, total_pages) in receiver {
                let progress = ImportProgress {
                    current_document: index + 1,
                    total_documents,
                    current_page,
                    total_pages,
                };
                let _ = receiver_app.emit("rancher://did-open-file-page", progress);
            }
        });

        let new_file = import_task.await.map_err(|e| e.to_string())?;
        let _ = update_progress_task.await;

        match new_file {
            Ok(file) => new_files.push(file),
            Err(e) => {
                let _ = app.emit("rancher://did-not-open-files", ());
                notify_error(&app, &e.to_string());
                return Err(e.to_string());
            }
        }
    }

    let mut app_state = app_state.lock().unwrap();

    app_state.add_source_files(new_files);

    let _ = app.emit("rancher://did-open-files", ());

    Ok(())
}

async fn open_files(app_handle: &AppHandle) -> Result<(), String> {
    let picked_paths = app_handle
        .dialog()
        .file()
        .add_filter("PDFs", &["pdf"])
        .blocking_pick_files();

    let Some(picked_paths) = picked_paths.map(|paths| {
        paths
            .into_iter()
            .filter_map(|f| match f {
                FilePath::Url(_) => None,
                FilePath::Path(p) => Some(p),
            })
            .collect::<Vec<_>>()
    }) else {
        return Ok(());
    };

    let result = add_files(app_handle.clone(), picked_paths).await;
    if let Err(e) = &result {
        notify_error(&app_handle, &e);
    }

    Ok(())
}

#[tauri::command]
async fn open_files_command(app_handle: AppHandle) {
    let result = open_files(&app_handle).await;

    if let Err(e) = &result {
        notify_error(&app_handle, &e);
    };
}

#[tauri::command]
fn load_project_command(
    app_handle: AppHandle,
    app_state: tauri::State<'_, Mutex<AppState>>,
) -> Result<AppState, String> {
    let Ok(state) = app_state.lock() else {
        let error = "Couldn't lock the application state".to_string();
        notify_error(&app_handle, &error);
        return Err(error);
    };

    Ok(state.clone())
}

async fn export(app_handle: &AppHandle, ordering: Vec<Selector>) -> Result<(), String> {
    let app_handle = app_handle.clone();

    let _ = tauri::async_runtime::spawn_blocking(move || {
        let path = app_handle
            .dialog()
            .file()
            .set_file_name("project.pdf")
            .add_filter("PDF", &["pdf"])
            .blocking_save_file();

        let path = match path {
            Some(FilePath::Path(p)) => p,
            _ => return,
        };

        let _ = app_handle.emit("rancher://will-export", ());

        let state = app_handle.state::<Mutex<AppState>>();
        let Ok(unlocked_state) = state.lock() else {
            let _ = app_handle.emit("rancher://did-not-export", ());
            return notify_error(&app_handle, "Couldn't lock the application state");
        };

        let Ok(mut document) = unlocked_state.project.export(&ordering).or_else(|e| {
            notify_error(
                &app_handle,
                format!("An error occurred while exporting the file: {}", e).as_str(),
            );
            let _ = app_handle.emit("rancher://did-not-export", ());
            Err(())
        }) else {
            return;
        };

        let Ok(_) = document.save(path).or_else(|e| {
            notify_error(
                &app_handle,
                format!("An error occurred while saving the file: {}", e).as_str(),
            );
            let _ = app_handle.emit("rancher://did-not-export", ());
            Err(())
        }) else {
            return;
        };

        let _ = app_handle.emit("rancher://did-export", ());
    })
    .await;

    Ok(())
}

fn notify_error(app_handle: &AppHandle, error: &str) {
    error!("{}", error);
    let _ = app_handle.emit("rancher://error", error);
}

#[tauri::command]
async fn export_command(app_handle: AppHandle, ordering: Vec<Selector>) -> Result<(), String> {
    let result = export(&app_handle, ordering).await;
    if let Err(e) = &result {
        notify_error(&app_handle, e);
    };
    result
}

async fn clear_project(app_handle: AppHandle) {
    let cloned_handle = app_handle.clone();
    let confirm = tauri::async_runtime::spawn_blocking(move || {
        cloned_handle
            .dialog()
            .message("Are you sure you want to clear the project?")
            .buttons(MessageDialogButtons::OkCancel)
            .blocking_show()
    })
    .await
    .unwrap();

    if !confirm {
        return;
    }

    let app_state = app_handle.state::<Mutex<AppState>>();
    let mut app_state = app_state.lock().unwrap();

    app_state.project.clear();

    let _ = app_handle.emit("rancher://did-clear-project", ());
}

#[tauri::command]
async fn clear_project_command(app_handle: AppHandle) {
    clear_project(app_handle).await;
}

#[tauri::command]
fn licenses_command(_app_handle: AppHandle) -> Result<Vec<License>, ()> {
    Ok(licenses::licenses())
}

#[tauri::command]
async fn preview_command(app_handle: AppHandle, ordering: Selector) -> Result<Page, ()> {
    let app_handle = app_handle.clone();

    let state = app_handle.state::<Mutex<AppState>>();
    let Ok(unlocked_state) = state.lock() else {
        notify_error(&app_handle, "Couldn't lock the application state");
        return Err(());
    };

    let Ok(page) = unlocked_state.project.preview(ordering).or_else(|e| {
        notify_error(
            &app_handle,
            format!("An error occurred while exporting the file: {}", e).as_str(),
        );
        Err(())
    }) else {
        return Err(());
    };

    Ok(page)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_window_state::Builder::new().build())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_dialog::init())
        .on_window_event(|window, event| match event {
            tauri::WindowEvent::DragDrop(drag_drop) => {
                if let tauri::DragDropEvent::Drop { paths, position: _ } = drag_drop {
                    let app_handle = window.app_handle().clone();
                    let paths = paths.clone();

                    tauri::async_runtime::spawn(async move {
                        let result = add_files(app_handle.clone(), paths).await;

                        if let Err(e) = &result {
                            notify_error(&app_handle, e);
                        };
                    });
                }
            }
            _ => {}
        })
        .on_menu_event(|app, event| {
            let id = event.id();

            if id == "open-file" {
                let app_handle = app.clone();
                tauri::async_runtime::spawn(async move {
                    let result = open_files(&app_handle).await;

                    if let Err(e) = &result {
                        return notify_error(&app_handle, &e);
                    }
                });
            }

            if id == "export" {
                let _ = app.emit("rancher://export-requested", ());
                return;
            }

            if id == "clear" {
                tauri::async_runtime::spawn(clear_project(app.clone()));
                return;
            }

            if id == "licenses" {
                let _ = app.emit("rancher://licenses-requested", ());
                return;
            }
        })
        .menu(|app| {
            let open_file =
                MenuItem::with_id(app, "open-file", "Open File…", true, Some("CmdOrCtrl+O"))?;
            let export = MenuItem::with_id(app, "export", "Export…", true, Some("CmdOrCtrl+E"))?;
            let clear = MenuItem::with_id(app, "clear", "Clear", true, Some("CmdOrCtrl+Shift+K"))?;
            let submenu = SubmenuBuilder::new(app, "File")
                .items(&[&open_file, &export, &clear])
                .build()?;

            let app_menu = SubmenuBuilder::new(app, "PDF Rancher")
                .about(Some(AboutMetadata::default()))
                .item(&MenuItem::with_id(
                    app,
                    "licenses",
                    "Licences",
                    true,
                    None::<&str>,
                )?)
                .separator()
                .hide()
                .quit()
                .build()?;

            let menu = MenuBuilder::new(app).build()?;
            menu.append(&app_menu)?;
            menu.append(&submenu)?;

            Ok(menu)
        })
        .plugin(
            tauri_plugin_log::Builder::new()
                .targets([
                    tauri_plugin_log::Target::new(tauri_plugin_log::TargetKind::Stdout),
                    tauri_plugin_log::Target::new(tauri_plugin_log::TargetKind::Webview),
                ])
                .build(),
        )
        .manage(Mutex::new(AppState::new()))
        .invoke_handler(tauri::generate_handler![
            open_files_command,
            load_project_command,
            export_command,
            clear_project_command,
            licenses_command,
            preview_command,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
