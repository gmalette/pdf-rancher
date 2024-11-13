mod project;
use crate::project::Project;
use log::info;
use serde::Serialize;
use project::SourceFile;
use std::sync::Mutex;
use tauri::Manager;
use tauri_api::dialog;
use tauri_api::dialog::Response;

#[derive(Debug, Clone, Serialize)]
struct AppState {
    document: Project
}

impl AppState {
    fn new() -> Self {
        Self {
            document: Project::new()
        }
    }

    fn add_source_files(&mut self, new_files: Vec<SourceFile>) {
        self.document.add_source_files(new_files);
    }
}

// Learn more about Tauri commands at https://tauri.app/develop/calling-rust/
#[tauri::command]
fn open_files(app_state: tauri::State<'_, Mutex<AppState>>) -> Result<AppState, String> {
    let response = dialog::select_multiple(Some("pdf"), None::<String>);

    let new_paths = match response {
        Ok(Response::Okay(file)) => { vec![file] }
        Ok(Response::OkayMultiple(files)) => { files }
        _ => { vec![] }
    };

    info!("Opening files: {:?}", new_paths);

    let new_files = new_paths.iter().map(|path| SourceFile::open(path)).collect::<Result<Vec<_>, _>>().map_err(|e| e.to_string())?;

    let mut app_state = app_state.lock().map_err(|_| "Couldn't lock the applicatoin state")?;

    app_state.add_source_files(new_files);

    Ok(app_state.clone())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .on_window_event(|window, event| match event {
            tauri::WindowEvent::DragDrop(drag_drop) => {
                if let tauri::DragDropEvent::Drop { paths: _, position: _ } = drag_drop {
                    let _app_state = window.state::<Mutex<AppState>>();

                    // let new_files = paths.iter().map(|path| SourceFile::open(path)).collect::<Result<Vec<_>, _>()?;
                    // new_files
                }
            }
            _ => {}
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
        .invoke_handler(tauri::generate_handler![open_files])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

