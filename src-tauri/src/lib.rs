mod project;
use crate::project::{Project, Selector};
use log::info;
use project::SourceFile;
use serde::Serialize;
use std::sync::Mutex;
use tauri::Emitter;
use tauri::Manager;
use tauri::menu::{Menu, MenuBuilder, SubmenuBuilder};
use tauri_api::dialog;
use tauri_api::dialog::Response;
use tauri::menu::*;
use tauri_api::path::app_dir;

#[derive(Debug, Clone, Serialize)]
struct AppState {
    project: Project
}

impl AppState {
    fn new() -> Self {
        Self {
            project: Project::new()
        }
    }

    fn add_source_files(&mut self, new_files: Vec<SourceFile>) {
        self.project.add_source_files(new_files);
    }
}

fn open_files(app_state: &mut AppState) -> Result<AppState, String> {
    let response = dialog::select_multiple(Some("pdf"), None::<String>);

    let new_paths = match response {
        Ok(Response::Okay(file)) => { vec![file] }
        Ok(Response::OkayMultiple(files)) => { files }
        _ => { vec![] }
    };

    info!("Opening files: {:?}", new_paths);

    let new_files = new_paths.iter().map(|path| SourceFile::open(path)).collect::<Result<Vec<_>, _>>().map_err(|e| e.to_string())?;

    app_state.add_source_files(new_files);

    Ok(app_state.clone())
}

#[tauri::command]
fn open_files_command(app_state: tauri::State<'_, Mutex<AppState>>) -> Result<AppState, String> {
    let mut app_state = app_state.lock().map_err(|_| "Couldn't lock the application state")?;
    open_files(&mut app_state)
}

#[tauri::command]
fn load_project_command(app_state: tauri::State<'_, Mutex<AppState>>) -> Result<AppState, String> {
    Ok(app_state.lock().map_err(|_| "Couldn't lock the application state")?.clone())
}

#[tauri::command]
fn export_command(app_state: tauri::State<'_, Mutex<AppState>>, ordering: Vec<Selector>) -> Result<(), String> {
    dbg!(&ordering);
    let response = dialog::save_file(Some("pdf"), None::<String>);

    let path = match response {
        Ok(Response::Okay(file)) => { file }
        _ => { return Ok(()); }
    };

    let project = &app_state.lock().map_err(|_| "Couldn't lock the application state")?.project;
    let mut document = project.export(&ordering).map_err(|e| e.to_string())?;
    document.save(&path).map_err(|e| e.to_string())?;

    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .on_window_event(|window, event| match event {
            tauri::WindowEvent::DragDrop(drag_drop) => {
                if let tauri::DragDropEvent::Drop { paths, position: _ } = drag_drop {
                    let _ = window.emit("files-will-open", ());
                    let app_state = window.state::<Mutex<AppState>>();

                    let new_files =
                        if let Ok(new_files) = paths.iter().map(|path| SourceFile::open(path)).collect::<Result<Vec<_>, _>>() {
                            new_files
                        } else {
                            vec![]
                        };

                    let mut app_state = app_state.lock().expect("Couldn't lock the application state");
                    app_state.add_source_files(new_files);

                    let _ = window.emit("files-did-open", ());
                }
            }
            _ => {}
        })
        .on_menu_event(|app, event| {
            let id = event.id();

            if id == "open-file" {
                let mut app_state = app.state::<Mutex<AppState>>().inner();
                let app_state = &mut app_state.lock().expect("Couldn't lock the application state");
                open_files(app_state);
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
        .setup(|app| {
            Ok(())
        })
        .plugin(tauri_plugin_log::Builder::new()
            .targets([
                tauri_plugin_log::Target::new(
                    tauri_plugin_log::TargetKind::Stdout,
                ),
                tauri_plugin_log::Target::new(
                    tauri_plugin_log::TargetKind::Webview,
                )
            ])
            .build())
        .manage(Mutex::new(AppState::new()))
        .invoke_handler(tauri::generate_handler![open_files_command, load_project_command, export_command])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

