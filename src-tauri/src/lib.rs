mod project;

use crate::project::{Project, Selector};
use log::error;
use project::SourceFile;
use serde::Serialize;
use std::path::PathBuf;
use std::sync::Mutex;
use tauri::menu::*;
use tauri::menu::{MenuBuilder, SubmenuBuilder};
use tauri::Manager;
use tauri::{AppHandle, Emitter};
use tauri_plugin_dialog::{DialogExt, FilePath};

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

fn add_files(
    app: &AppHandle,
    app_state: &Mutex<AppState>,
    paths: &Vec<PathBuf>,
) -> Result<(), String> {
    if paths.is_empty() {
        return Ok(());
    };

    let _ = app.emit("rancher://will-open-files", ());

    let new_files = paths
        .iter()
        .map(|path| SourceFile::open(path))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;

    let mut app_state = app_state
        .lock()
        .map_err(|_| "Couldn't lock the application state")?;

    app_state.add_source_files(new_files);

    let _ = app.emit("rancher://did-open-files", ());

    Ok(())
}

fn open_files(app_handle: &AppHandle) -> Result<(), String> {
    use tauri_plugin_dialog::DialogExt;

    let app_handle = app_handle.clone();
    app_handle
        .dialog()
        .file()
        .add_filter("PDFs", &["pdf"])
        .pick_files(move |picked_paths| {
            let Some(picked_paths) = picked_paths.map(|paths| {
                paths
                    .into_iter()
                    .filter_map(|f| match f {
                        FilePath::Url(_) => None,
                        FilePath::Path(p) => Some(p),
                    })
                    .collect::<Vec<_>>()
            }) else {
                return;
            };

            let app_state = app_handle.state::<Mutex<AppState>>();

            if let Err(e) = add_files(&app_handle, app_state.inner(), &picked_paths) {
                notify_error(&app_handle, &e);
            }
        });

    Ok(())
}

#[tauri::command]
fn open_files_command(app_handle: AppHandle) {
    if let Err(e) = open_files(&app_handle) {
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

fn export(app_handle: &AppHandle, ordering: Vec<Selector>) -> Result<(), String> {
    let _ = app_handle.emit("rancher://will-export", ());

    let app_handle = app_handle.clone();

    app_handle
        .dialog()
        .file()
        .set_file_name("project.pdf")
        .save_file(move |path| {
            let path = match path {
                Some(FilePath::Path(p)) => p,
                _ => return,
            };

            let state = app_handle.state::<Mutex<AppState>>();
            let Ok(unlocked_state) = state.lock() else {
                return notify_error(&app_handle, "Couldn't lock the application state");
            };

            let Ok(mut document) = unlocked_state.project.export(&ordering).or_else(|e| {
                notify_error(&app_handle, format!("An error occurred while exporting the file: {}", e).as_str());
                Err(())
            }) else { return };

            let Ok(_) = document.save(path).or_else(|e| {
                notify_error(&app_handle, format!("An error occurred while saving the file: {}", e).as_str());
                Err(())
            }) else { return };

            let _ = app_handle.emit("rancher://did-export", ());
        });

    Ok(())
}

fn notify_error(app_handle: &AppHandle, error: &str) {
    error!("{}", error);
    let _ = app_handle.emit("rancher://error", error);
}

#[tauri::command]
fn export_command(app_handle: AppHandle, ordering: Vec<Selector>) {
    if let Err(e) = export(&app_handle, ordering) {
        notify_error(&app_handle, &e); };
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
                    let app_state = window.state::<Mutex<AppState>>();

                    if let Err(e) = add_files(window.app_handle(), app_state.inner(), paths) {
                        notify_error(window.app_handle(), &e);
                    }
                }
            }
            _ => {}
        })
        .on_menu_event(|app, event| {
            let id = event.id();

            if id == "open-file" {
                if let Err(e) = open_files(&app) {
                    return notify_error(&app, &e);
                }
            }

            if id == "export" {
                let _ = app.emit("rancher://export-requested", ());
                return;
            }
        })
        .menu(|app| {
            let submenu = SubmenuBuilder::new(app, "File")
                .text("open-file", "Open File…")
                .text("export", "Export…")
                .build()?;

            let app_menu = SubmenuBuilder::new(app, "PDF Rancher")
                .about(Some(AboutMetadata::default()))
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
            export_command
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
