mod python;
mod project;
mod fs;
mod engine_http;

use std::path::PathBuf;
use tauri::{Emitter, State};
use std::sync::Arc;
use tokio::sync::Mutex;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct ProjectInfo {
    pub name: String,
    pub root: String,
}

/// Application state to track the current project and engine server
pub struct AppState {
    pub current_project: Mutex<Option<project::TetherProject>>,
    pub engine_server: Arc<Mutex<Option<engine_http::EngineServer>>>,
}

// Learn more about Tauri commands at https://tauri.app/develop/calling-rust/
#[tauri::command]
fn greet(name: &str) -> String {
    format!("Hello, {}! You've been greeted from Rust!", name)
}



#[tauri::command]
async fn check_uv_installed() -> Result<bool, String> {
    match python::check_uv_available() {
        Ok(_) => Ok(true),
        Err(_) => Ok(false),
    }
}

#[tauri::command]
async fn install_uv() -> Result<String, String> {
    python::install_uv()
        .await
        .map_err(|e| e.to_string())?;

    Ok("uv installed successfully".to_string())
}

#[tauri::command]
async fn ensure_uv() -> Result<String, String> {
    let uv_path = python::ensure_uv()
        .await
        .map_err(|e| e.to_string())?;

    Ok(uv_path.to_string_lossy().to_string())
}

#[tauri::command]
async fn init_python_env(project_path: String, state: State<'_, AppState>) -> Result<String, String> {
    let path = PathBuf::from(&project_path);

    // Load project to get package name
    let current = state.current_project.lock().await;
    let package_name = if let Some(project) = current.as_ref() {
        if project.root == path {
            project.package_name.clone()
        } else {
            // Load project if path doesn't match current
            drop(current); // Release lock before loading
            let loaded_project = project::load_project(&path).map_err(|e| e.to_string())?;
            loaded_project.package_name
        }
    } else {
        // No current project, load it
        drop(current); // Release lock before loading
        let loaded_project = project::load_project(&path).map_err(|e| e.to_string())?;
        loaded_project.package_name
    };

    python::init_project(&path, &package_name)
        .await
        .map_err(|e| e.to_string())?;

    Ok("Python environment initialized successfully".to_string())
}

#[tauri::command]
async fn ensure_python_venv(project_path: String, state: State<'_, AppState>) -> Result<String, String> {
    let path = PathBuf::from(&project_path);

    // Load project to get package name
    let current = state.current_project.lock().await;
    let package_name = if let Some(project) = current.as_ref() {
        if project.root == path {
            project.package_name.clone()
        } else {
            drop(current);
            let loaded_project = project::load_project(&path).map_err(|e| e.to_string())?;
            loaded_project.package_name
        }
    } else {
        drop(current);
        let loaded_project = project::load_project(&path).map_err(|e| e.to_string())?;
        loaded_project.package_name
    };

    let venv_path = python::ensure_venv(&path, &package_name)
        .await
        .map_err(|e| e.to_string())?;

    Ok(venv_path.to_string_lossy().to_string())
}

#[tauri::command]
async fn install_python_package(project_path: String, package: String) -> Result<String, String> {
    let path = PathBuf::from(project_path);

    python::install_package(&path, &package)
        .await
        .map_err(|e| e.to_string())?;

    Ok(format!("Package {} installed successfully", package))
}

#[tauri::command]
async fn install_python_packages(project_path: String, packages: Vec<String>) -> Result<String, String> {
    let path = PathBuf::from(project_path);
    let package_refs: Vec<&str> = packages.iter().map(|s| s.as_str()).collect();

    python::install_packages(&path, &package_refs)
        .await
        .map_err(|e| e.to_string())?;

    Ok(format!("Packages installed successfully"))
}

#[tauri::command]
async fn run_python_code(project_path: String, code: String, state: State<'_, AppState>) -> Result<String, String> {
    let path = PathBuf::from(&project_path);

    // Load project to get package name
    let current = state.current_project.lock().await;
    let package_name = if let Some(project) = current.as_ref() {
        if project.root == path {
            project.package_name.clone()
        } else {
            drop(current);
            let loaded_project = project::load_project(&path).map_err(|e| e.to_string())?;
            loaded_project.package_name
        }
    } else {
        drop(current);
        let loaded_project = project::load_project(&path).map_err(|e| e.to_string())?;
        loaded_project.package_name
    };

    let output = python::run_python_command(&path, &package_name, &["-c", &code])
        .await
        .map_err(|e| e.to_string())?;

    Ok(output)
}

#[tauri::command]
async fn create_project(project_path: String, project_name: String, state: State<'_, AppState>) -> Result<ProjectInfo, String> {
    let path = PathBuf::from(&project_path);

    let project = project::create_new_project(&path, &project_name)
        .await
        .map_err(|e| e.to_string())?;

    let project_info = ProjectInfo {
        name: project.name.clone(),
        root: project.root.to_string_lossy().to_string(),
    };

    // Set as current project
    let mut current = state.current_project.lock().await;
    *current = Some(project);

    Ok(project_info)
}

#[tauri::command]
async fn open_folder(folder_path: String, state: State<'_, AppState>) -> Result<ProjectInfo, String> {
    let path = PathBuf::from(&folder_path);

    let project = project::open_folder(&path)
        .await
        .map_err(|e| e.to_string())?;

    let project_info = ProjectInfo {
        name: project.name.clone(),
        root: project.root.to_string_lossy().to_string(),
    };

    // Set as current project
    let mut current = state.current_project.lock().await;
    *current = Some(project);

    Ok(project_info)
}

#[tauri::command]
async fn load_project(project_path: String, state: State<'_, AppState>) -> Result<ProjectInfo, String> {
    let path = PathBuf::from(&project_path);

    let project = project::load_project(&path)
        .map_err(|e| e.to_string())?;

    let project_info = ProjectInfo {
        name: project.name.clone(),
        root: project.root.to_string_lossy().to_string(),
    };

    // Set as current project
    let mut current = state.current_project.lock().await;
    *current = Some(project);

    Ok(project_info)
}

#[tauri::command]
async fn get_current_project(state: State<'_, AppState>) -> Result<Option<ProjectInfo>, String> {
    let current = state.current_project.lock().await;

    if let Some(project) = current.as_ref() {
        Ok(Some(ProjectInfo {
            name: project.name.clone(),
            root: project.root.to_string_lossy().to_string(),
        }))
    } else {
        Ok(None)
    }
}

#[tauri::command]
async fn set_project_root(project_path: String, state: State<'_, AppState>) -> Result<(), String> {
    let path = PathBuf::from(&project_path);
    let project = project::load_project(&path)
        .map_err(|e| e.to_string())?;

    let mut current = state.current_project.lock().await;
    *current = Some(project);
    Ok(())
}

#[tauri::command]
async fn get_project_root(state: State<'_, AppState>) -> Result<Option<String>, String> {
    let current = state.current_project.lock().await;
    Ok(current.as_ref().map(|p| p.root.to_string_lossy().to_string()))
}

#[tauri::command]
async fn list_files(directory_path: String) -> Result<Vec<fs::FileEntry>, String> {
    let path = PathBuf::from(directory_path);
    fs::list_directory(&path).map_err(|e| e.to_string())
}

#[tauri::command]
async fn create_workbook(
    workbook_path: String,
    workbook_name: String,
) -> Result<String, String> {
    let path = PathBuf::from(workbook_path);
    fs::create_workbook(&path, &workbook_name).map_err(|e| e.to_string())
}

#[tauri::command]
async fn read_workbook(workbook_path: String) -> Result<String, String> {
    let path = PathBuf::from(workbook_path);
    fs::read_workbook(&path).map_err(|e| e.to_string())
}

#[tauri::command]
async fn save_workbook(
    workbook_path: String,
    content: String,
) -> Result<(), String> {
    let path = PathBuf::from(workbook_path);
    fs::save_workbook(&path, &content).map_err(|e| e.to_string())
}

#[tauri::command]
async fn read_file(file_path: String) -> Result<String, String> {
    let path = PathBuf::from(file_path);
    fs::read_file(&path).map_err(|e| e.to_string())
}

#[tauri::command]
async fn save_file(
    file_path: String,
    content: String,
) -> Result<(), String> {
    let path = PathBuf::from(file_path);
    fs::save_file(&path, &content).map_err(|e| e.to_string())
}

#[tauri::command]
async fn rename_file(
    old_path: String,
    new_name: String,
) -> Result<String, String> {
    let path = PathBuf::from(old_path);
    fs::rename_file(&path, &new_name).map_err(|e| e.to_string())
}

#[tauri::command]
async fn delete_file(file_path: String) -> Result<(), String> {
    let path = PathBuf::from(file_path);
    fs::delete_file(&path).map_err(|e| e.to_string())
}

#[tauri::command]
async fn duplicate_workbook(
    source_path: String,
    new_name: String,
) -> Result<String, String> {
    let path = PathBuf::from(source_path);
    fs::duplicate_workbook(&path, &new_name).map_err(|e| e.to_string())
}

#[tauri::command]
async fn ensure_engine_server(state: State<'_, AppState>) -> Result<(), String> {
    // Use async mutex to hold lock across await - prevents race condition
    let mut server = state.engine_server.lock().await;

    if server.is_none() {
        println!("Starting engine server...");
        let es = engine_http::EngineServer::start()
            .await
            .map_err(|e| e.to_string())?;

        *server = Some(es);
        println!("Engine server started successfully");
    }

    Ok(())
}

#[tauri::command]
async fn start_engine(
    workbook_path: String,
    project_path: String,
    _engine_name: Option<String>,
    state: State<'_, AppState>,
) -> Result<(), String> {
    // Ensure engine server is running
    ensure_engine_server(state.clone()).await?;

    let project_root = PathBuf::from(&project_path);

    // Get package name from current project or load it
    let current = state.current_project.lock().await;
    let package_name = if let Some(project) = current.as_ref() {
        if project.root == project_root {
            project.package_name.clone()
        } else {
            drop(current);
            let loaded_project = project::load_project(&project_root).map_err(|e| e.to_string())?;
            loaded_project.package_name
        }
    } else {
        drop(current);
        let loaded_project = project::load_project(&project_root).map_err(|e| e.to_string())?;
        loaded_project.package_name
    };

    // Get venv path
    let venv_path = python::get_venv_path(&project_root, &package_name)
        .map_err(|e| e.to_string())?;

    // Get the port
    let port = {
        let server = state.engine_server.lock().await;
        server.as_ref().map(|s| s.port).ok_or("Engine server not initialized")?
    };

    // Now call the static method without holding any locks
    engine_http::EngineServer::start_engine_http(port, &workbook_path, &project_root, &venv_path)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn execute_cell(
    workbook_path: String,
    code: String,
    state: State<'_, AppState>,
) -> Result<engine_http::ExecutionResult, String> {
    let port = {
        let server = state.engine_server.lock().await;
        server.as_ref().map(|s| s.port).ok_or("Engine server not initialized")?
    };

    engine_http::EngineServer::execute_http(port, &workbook_path, &code)
        .await
        .map_err(|e| e.to_string())
}

// Simple hash function to match JavaScript implementation
fn hash_string(s: &str) -> u32 {
    let mut hash: i32 = 0;
    for c in s.chars() {
        hash = ((hash << 5).wrapping_sub(hash)).wrapping_add(c as i32);
    }
    hash.abs() as u32
}

#[derive(serde::Serialize)]
struct StreamExecutionResult {
    success: bool,
    execution_count: Option<i32>,
}

#[tauri::command]
async fn execute_cell_stream(
    workbook_path: String,
    code: String,
    window: tauri::Window,
    state: State<'_, AppState>,
) -> Result<StreamExecutionResult, String> {
    let port = {
        let server = state.engine_server.lock().await;
        server.as_ref().map(|s| s.port).ok_or("Engine server not initialized")?
    };

    // Create event name using hash of workbook path
    let event_name = format!("cell-output-{}", hash_string(&workbook_path));
    println!("Emitting to event: {}", event_name);

    let (success, execution_count) = engine_http::EngineServer::execute_stream(
        port,
        &workbook_path,
        &code,
        move |output| {
            // Emit event to frontend with output
            let _ = window.emit(&event_name, output);
        }
    )
    .await
    .map_err(|e| e.to_string())?;

    Ok(StreamExecutionResult {
        success,
        execution_count,
    })
}

#[tauri::command]
async fn stop_engine(
    workbook_path: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let port = {
        let server = state.engine_server.lock().await;
        server.as_ref().map(|s| s.port).ok_or("Engine server not initialized")?
    };

    engine_http::EngineServer::stop_engine_http(port, &workbook_path)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn interrupt_engine(
    workbook_path: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let port = {
        let server = state.engine_server.lock().await;
        server.as_ref().map(|s| s.port).ok_or("Engine server not initialized")?
    };

    engine_http::EngineServer::interrupt_engine_http(port, &workbook_path)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn complete_code(
    workbook_path: String,
    code: String,
    cursor_pos: i32,
    state: State<'_, AppState>,
) -> Result<engine_http::CompletionResult, String> {
    let port = {
        let server = state.engine_server.lock().await;
        server.as_ref().map(|s| s.port).ok_or("Engine server not initialized")?
    };

    engine_http::EngineServer::complete_code_http(port, &workbook_path, &code, cursor_pos)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn restart_engine(
    workbook_path: String,
    project_path: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    // Ensure engine server is running
    ensure_engine_server(state.clone()).await?;

    let project_root = PathBuf::from(&project_path);

    // Get package name from current project or load it
    let current = state.current_project.lock().await;
    let package_name = if let Some(project) = current.as_ref() {
        if project.root == project_root {
            project.package_name.clone()
        } else {
            drop(current);
            let loaded_project = project::load_project(&project_root).map_err(|e| e.to_string())?;
            loaded_project.package_name
        }
    } else {
        drop(current);
        let loaded_project = project::load_project(&project_root).map_err(|e| e.to_string())?;
        loaded_project.package_name
    };

    // Get venv path
    let venv_path = python::get_venv_path(&project_root, &package_name)
        .map_err(|e| e.to_string())?;

    // Get the port
    let port = {
        let server = state.engine_server.lock().await;
        server.as_ref().map(|s| s.port).ok_or("Engine server not initialized")?
    };

    // Now call the static method without holding any locks
    engine_http::EngineServer::restart_engine_http(port, &workbook_path, &project_root, &venv_path)
        .await
        .map_err(|e| e.to_string())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_window_state::Builder::default().build())
        .manage(AppState {
            current_project: Mutex::new(None),
            engine_server: Arc::new(Mutex::new(None)),
        })
        .invoke_handler(tauri::generate_handler![
            greet,
            check_uv_installed,
            install_uv,
            ensure_uv,
            create_project,
            open_folder,
            load_project,
            get_current_project,
            init_python_env,
            ensure_python_venv,
            install_python_package,
            install_python_packages,
            run_python_code,
            set_project_root,
            get_project_root,
            list_files,
            create_workbook,
            read_workbook,
            save_workbook,
            read_file,
            save_file,
            rename_file,
            delete_file,
            duplicate_workbook,
            ensure_engine_server,
            start_engine,
            execute_cell,
            execute_cell_stream,
            complete_code,
            stop_engine,
            interrupt_engine,
            restart_engine,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
