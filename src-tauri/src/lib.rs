mod project;
use crate::project::{Project, Selector};
use project::SourceFile;
use serde::Serialize;
use std::sync::Mutex;
use tauri::menu::*;
use tauri::menu::{MenuBuilder, SubmenuBuilder};
use tauri::Manager;
use tauri::{AppHandle, Emitter};
use tauri_plugin_dialog::{DialogExt, FilePath};
use tauri_plugin_notification::NotificationExt;

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

fn open_files(app_handle: AppHandle) -> Result<(), String> {
    use tauri_plugin_dialog::DialogExt;

    app_handle
        .dialog()
        .file()
        .add_filter("PDFs", &["pdf"])
        .pick_files(move |picked_paths| {
            let picked_paths = picked_paths.map(|paths| {
                paths
                    .into_iter()
                    .filter_map(|f| match f {
                        FilePath::Url(_) => None,
                        FilePath::Path(p) => Some(p),
                    })
                    .collect::<Vec<_>>()
            });

            let new_paths = if let Some(new_paths) = picked_paths {
                new_paths
            } else {
                return;
            };

            let new_files = new_paths
                .iter()
                .map(|path| SourceFile::open(path))
                .collect::<Result<Vec<_>, _>>()
                .map_err(|e| e.to_string());

            let app_state = app_handle.state::<Mutex<AppState>>();
            let app_state = app_state
                .lock()
                .map_err(|_| "Couldn't lock the application state");

            if let (Ok(new_files), Ok(mut app_state)) = (new_files, app_state) {
                app_state.add_source_files(new_files);

                let _ = app_handle.emit("files-did-open", ());
            };
        });

    Ok(())
}

#[tauri::command]
fn open_files_command(app_handle: AppHandle) -> Result<(), String> {
    open_files(app_handle)
}

#[tauri::command]
fn load_project_command(app_state: tauri::State<'_, Mutex<AppState>>) -> Result<AppState, String> {
    Ok(app_state
        .lock()
        .map_err(|_| "Couldn't lock the application state")?
        .clone())
}

fn notify_error(app_handle: &AppHandle, message: &str) {
    use tauri_plugin_notification::NotificationExt;
    app_handle.notification()
        .builder()
        .title("PDF Rancher: Error")
        .body(message)
        .show()
        .unwrap();
}

#[tauri::command]
fn export_command(app_handle: AppHandle, ordering: Vec<Selector>) -> Result<(), String> {
    let _ = app_handle.emit("rancher://will-export", ());

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
                return notify_error(&app_handle, "An error occurred while exporting the file");
            };

            let Ok(mut document) = unlocked_state
                .project
                .export(&ordering) else {
                return notify_error(&app_handle, "An error occurred while exporting the file");
            };

            let Ok(_) = document.save(path) else {
                return notify_error(&app_handle, "An error occurred while saving the file");
            };
        });

    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_dialog::init())
        .on_window_event(|window, event| match event {
            tauri::WindowEvent::DragDrop(drag_drop) => {
                if let tauri::DragDropEvent::Drop { paths, position: _ } = drag_drop {
                    let _ = window.emit("rancher:will-open-files", ());
                    let app_state = window.state::<Mutex<AppState>>();

                    let new_files = if let Ok(new_files) = paths
                        .iter()
                        .map(|path| SourceFile::open(path))
                        .collect::<Result<Vec<_>, _>>()
                    {
                        new_files
                    } else {
                        vec![]
                    };

                    let mut app_state = app_state
                        .lock()
                        .expect("Couldn't lock the application state");
                    app_state.add_source_files(new_files);

                    let _ = window.emit("rancher://did-open-files", ());
                }
            }
            _ => {}
        })
        .on_menu_event(|app, event| {
            let id = event.id();

            if id == "open-file" {
                let _ = open_files(app.clone());
                return;
            }

            if id == "export" {
                let _ = app.emit("export-requested", ());
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
