pub mod python;
pub mod project;
mod fs;
pub mod engine_http;
mod secrets;
pub mod scheduler;
pub mod cli_install;

#[cfg(target_os = "macos")]
mod local_auth_macos;

use std::path::PathBuf;
use tauri::{Emitter, Manager, State};
use std::sync::Arc;
use tokio::sync::Mutex;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct ProjectInfo {
    pub name: String,
    pub root: String,
}

/// Application state to track the current project, engine server, and secrets manager
pub struct AppState {
    pub current_project: Mutex<Option<project::TetherProject>>,
    pub engine_server: Arc<Mutex<Option<engine_http::EngineServer>>>,
    pub secrets_manager: Arc<Mutex<Option<secrets::SecretsManager>>>,
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
async fn read_file_binary(file_path: String) -> Result<Vec<u8>, String> {
    let path = PathBuf::from(file_path);
    fs::read_file_binary(&path).map_err(|e| e.to_string())
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
async fn create_new_file(
    parent_path: String,
    file_name: String,
    initial_content: Option<String>,
) -> Result<String, String> {
    let path = PathBuf::from(parent_path);
    fs::create_new_file(&path, &file_name, initial_content.as_deref())
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn create_new_folder(
    parent_path: String,
    folder_name: String,
) -> Result<String, String> {
    let path = PathBuf::from(parent_path);
    fs::create_new_folder(&path, &folder_name).map_err(|e| e.to_string())
}

#[tauri::command]
async fn get_file_info(file_path: String) -> Result<fs::FileInfo, String> {
    let path = PathBuf::from(file_path);
    fs::get_file_info(&path).map_err(|e| e.to_string())
}

#[tauri::command]
async fn reveal_in_finder(file_path: String) -> Result<(), String> {
    let path = PathBuf::from(file_path);
    fs::reveal_in_finder(&path).map_err(|e| e.to_string())
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
async fn save_dropped_file(
    project_root: String,
    file_name: String,
    file_content: Vec<u8>,
) -> Result<String, String> {
    let path = PathBuf::from(project_root);
    fs::save_dropped_file(&path, &file_name, &file_content).map_err(|e| e.to_string())
}

#[tauri::command]
async fn save_dropped_folder(
    project_root: String,
    folder_path: String,
) -> Result<String, String> {
    let proj_path = PathBuf::from(project_root);
    let folder = PathBuf::from(folder_path);
    fs::save_dropped_folder(&proj_path, &folder).map_err(|e| e.to_string())
}

/// Handle a dropped file or folder - detects type and saves appropriately
/// This avoids needing frontend fs permissions by doing everything in Rust
#[tauri::command]
async fn handle_dropped_item(
    project_root: String,
    dropped_path: String,
) -> Result<String, String> {
    let proj_path = PathBuf::from(&project_root);
    let item_path = PathBuf::from(&dropped_path);

    fs::handle_dropped_item(&proj_path, &item_path).map_err(|e| e.to_string())
}

#[tauri::command]
async fn ensure_engine_server(state: State<'_, AppState>) -> Result<(), String> {
    // Use async mutex to hold lock across await - prevents race condition
    let mut server = state.engine_server.lock().await;

    if server.is_none() {
        log::info!("Starting engine server...");
        let es = engine_http::EngineServer::start()
            .await
            .map_err(|e| {
                log::error!("Failed to start engine server: {}", e);
                e.to_string()
            })?;

        *server = Some(es);
        log::info!("Engine server started successfully");
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
    log::debug!("Emitting to event: {}", event_name);

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

// Secrets management commands

/// Helper function to get or create the SecretsManager for a project
async fn get_secrets_manager(
    project_path: &PathBuf,
    state: &State<'_, AppState>,
) -> Result<Arc<Mutex<Option<secrets::SecretsManager>>>, String> {
    let mut manager_guard = state.secrets_manager.lock().await;

    // Check if we need to create a new manager or if the existing one is for a different project
    let needs_new = match manager_guard.as_ref() {
        None => true,
        Some(_manager) => {
            // For simplicity, always recreate if path changes
            // In a more sophisticated version, we could track which project the manager is for
            false
        }
    };

    if needs_new {
        let manager = secrets::SecretsManager::new(project_path)
            .map_err(|e| e.to_string())?;
        *manager_guard = Some(manager);
    }

    Ok(state.secrets_manager.clone())
}

#[tauri::command]
async fn add_secret(
    project_path: String,
    key: String,
    value: String,
    state: State<'_, AppState>,
) -> Result<secrets::Secret, String> {
    let path = PathBuf::from(project_path);
    let manager_arc = get_secrets_manager(&path, &state).await?;
    let manager_guard = manager_arc.lock().await;
    let manager = manager_guard.as_ref().ok_or("No secrets manager")?;

    manager.add_secret(&key, &value)
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn get_secret(
    project_path: String,
    key: String,
    state: State<'_, AppState>,
) -> Result<String, String> {
    let path = PathBuf::from(project_path);
    let manager_arc = get_secrets_manager(&path, &state).await?;
    let manager_guard = manager_arc.lock().await;
    let manager = manager_guard.as_ref().ok_or("No secrets manager")?;

    manager.get_secret(&key)
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn get_secret_authenticated(
    project_path: String,
    key: String,
    state: State<'_, AppState>,
) -> Result<String, String> {
    let path = PathBuf::from(project_path);
    let manager_arc = get_secrets_manager(&path, &state).await?;
    let manager_guard = manager_arc.lock().await;
    let manager = manager_guard.as_ref().ok_or("No secrets manager")?;

    manager.get_secret_authenticated(&key)
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn list_secrets(
    project_path: String,
    state: State<'_, AppState>,
) -> Result<Vec<secrets::Secret>, String> {
    let path = PathBuf::from(project_path);
    let manager_arc = get_secrets_manager(&path, &state).await?;
    let manager_guard = manager_arc.lock().await;
    let manager = manager_guard.as_ref().ok_or("No secrets manager")?;

    manager.list_secrets()
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn update_secret(
    project_path: String,
    key: String,
    value: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let path = PathBuf::from(project_path);
    let manager_arc = get_secrets_manager(&path, &state).await?;
    let manager_guard = manager_arc.lock().await;
    let manager = manager_guard.as_ref().ok_or("No secrets manager")?;

    manager.update_secret(&key, &value)
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn delete_secret(
    project_path: String,
    key: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let path = PathBuf::from(project_path);
    let manager_arc = get_secrets_manager(&path, &state).await?;
    let manager_guard = manager_arc.lock().await;
    let manager = manager_guard.as_ref().ok_or("No secrets manager")?;

    manager.delete_secret(&key)
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn get_all_secrets(
    project_path: String,
    state: State<'_, AppState>,
) -> Result<Vec<(String, String)>, String> {
    let path = PathBuf::from(project_path);
    let manager_arc = get_secrets_manager(&path, &state).await?;
    let manager_guard = manager_arc.lock().await;
    let manager = manager_guard.as_ref().ok_or("No secrets manager")?;

    manager.get_all_secrets_with_values()
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn import_secrets_from_env(
    project_path: String,
    env_file_path: String,
    state: State<'_, AppState>,
) -> Result<Vec<String>, String> {
    let project = PathBuf::from(project_path);
    let env_path = PathBuf::from(env_file_path);

    let manager_arc = get_secrets_manager(&project, &state).await?;
    let manager_guard = manager_arc.lock().await;
    let manager = manager_guard.as_ref().ok_or("No secrets manager")?;

    manager.import_from_env(&env_path)
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn authenticate_secrets_access(
    project_path: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let path = PathBuf::from(project_path);
    let manager_arc = get_secrets_manager(&path, &state).await?;
    let manager_guard = manager_arc.lock().await;
    let manager = manager_guard.as_ref().ok_or("No secrets manager")?;

    // This will trigger Touch ID by accessing the keychain
    // Even if there are no secrets, this will ensure the encryption key exists
    // and can be accessed (which requires Touch ID on macOS)
    manager.ensure_encryption_key_accessible()
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn check_secrets_session(
    project_path: String,
    state: State<'_, AppState>,
) -> Result<bool, String> {
    let path = PathBuf::from(project_path);
    let manager_arc = get_secrets_manager(&path, &state).await?;
    let manager_guard = manager_arc.lock().await;
    let manager = manager_guard.as_ref().ok_or("No secrets manager")?;

    Ok(manager.is_session_valid())
}

#[tauri::command]
async fn lock_secrets_session(
    project_path: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let path = PathBuf::from(project_path);
    let manager_arc = get_secrets_manager(&path, &state).await?;
    let manager_guard = manager_arc.lock().await;
    let manager = manager_guard.as_ref().ok_or("No secrets manager")?;

    manager.lock_session();
    Ok(())
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CellOutputScanResult {
    pub cell_indices: Vec<usize>,
    pub has_secrets: bool,
}

#[tauri::command]
async fn scan_outputs_for_secrets(
    project_path: String,
    cells_json: String,
    state: State<'_, AppState>,
) -> Result<CellOutputScanResult, String> {
    let path = PathBuf::from(project_path);
    let manager_arc = get_secrets_manager(&path, &state).await?;
    let manager_guard = manager_arc.lock().await;
    let manager = manager_guard.as_ref().ok_or("No secrets manager")?;

    // Get all secrets with their values
    let secrets = manager.get_all_secrets_with_values()
        .map_err(|e| e.to_string())?;

    // If no secrets, nothing to scan for
    if secrets.is_empty() {
        return Ok(CellOutputScanResult {
            cell_indices: vec![],
            has_secrets: false,
        });
    }

    // Parse the cells JSON
    let cells: Vec<serde_json::Value> = serde_json::from_str(&cells_json)
        .map_err(|e| format!("Failed to parse cells: {}", e))?;

    let mut cell_indices_with_secrets = Vec::new();

    // Scan each cell's outputs
    for (index, cell) in cells.iter().enumerate() {
        if let Some(outputs) = cell.get("outputs").and_then(|o| o.as_array()) {
            if outputs.is_empty() {
                continue;
            }

            // Convert outputs to JSON string for easier searching
            let outputs_str = serde_json::to_string(outputs)
                .map_err(|e| format!("Failed to serialize outputs: {}", e))?;

            // Check if any secret value appears in the outputs
            let mut contains_secret = false;
            for (_key, value) in &secrets {
                // Only check non-empty secrets
                if !value.is_empty() && outputs_str.contains(value) {
                    contains_secret = true;
                    break;
                }
            }

            if contains_secret {
                cell_indices_with_secrets.push(index);
            }
        }
    }

    Ok(CellOutputScanResult {
        cell_indices: cell_indices_with_secrets.clone(),
        has_secrets: !cell_indices_with_secrets.is_empty(),
    })
}

#[tauri::command]
async fn test_secrets_loading(
    project_path: String,
) -> Result<String, String> {
    let path = PathBuf::from(&project_path);

    println!("=== TESTING SECRETS LOADING ===");
    println!("Project path: {}", path.display());

    match secrets::SecretsManager::new(&path) {
        Ok(manager) => {
            println!("✓ SecretsManager created");

            match manager.list_secrets() {
                Ok(secrets) => {
                    println!("✓ Found {} secrets in database", secrets.len());
                    for secret in &secrets {
                        println!("  - {}", secret.key);
                    }

                    match manager.get_all_secrets_with_values() {
                        Ok(values) => {
                            println!("✓ Successfully decrypted all secrets");
                            Ok(format!("Success! Found {} secrets: {}",
                                values.len(),
                                values.iter().map(|(k, _)| k.as_str()).collect::<Vec<_>>().join(", ")
                            ))
                        }
                        Err(e) => {
                            println!("✗ Failed to decrypt secrets: {}", e);
                            Err(format!("Failed to decrypt secrets: {}", e))
                        }
                    }
                }
                Err(e) => {
                    println!("✗ Failed to list secrets: {}", e);
                    Err(format!("Failed to list secrets: {}", e))
                }
            }
        }
        Err(e) => {
            println!("✗ Failed to create SecretsManager: {}", e);
            Err(format!("Failed to create SecretsManager: {}", e))
        }
    }
}

#[tauri::command]
async fn get_logs_directory(app: tauri::AppHandle) -> Result<String, String> {
    let logs_dir = app.path().app_log_dir()
        .map_err(|e| format!("Failed to get log directory: {}", e))?;

    // Ensure the logs directory exists
    std::fs::create_dir_all(&logs_dir)
        .map_err(|e| format!("Failed to create logs directory: {}", e))?;

    Ok(logs_dir.to_string_lossy().to_string())
}

#[tauri::command]
async fn open_logs_folder(app: tauri::AppHandle) -> Result<(), String> {
    let logs_dir = app.path().app_log_dir()
        .map_err(|e| format!("Failed to get log directory: {}", e))?;

    // Ensure the logs directory exists
    std::fs::create_dir_all(&logs_dir)
        .map_err(|e| format!("Failed to create logs directory: {}", e))?;

    // Open the directory in the system file manager
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg(&logs_dir)
            .spawn()
            .map_err(|e| format!("Failed to open logs folder: {}", e))?;
    }

    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("explorer")
            .arg(&logs_dir)
            .spawn()
            .map_err(|e| format!("Failed to open logs folder: {}", e))?;
    }

    #[cfg(target_os = "linux")]
    {
        std::process::Command::new("xdg-open")
            .arg(&logs_dir)
            .spawn()
            .map_err(|e| format!("Failed to open logs folder: {}", e))?;
    }

    Ok(())
}

#[tauri::command]
async fn get_recent_logs(app: tauri::AppHandle, lines: Option<usize>) -> Result<String, String> {
    let logs_dir = app.path().app_log_dir()
        .map_err(|e| format!("Failed to get log directory: {}", e))?;

    // tauri-plugin-log creates log files with timestamp suffixes
    // Look for the most recent log file
    let log_file = logs_dir.join("tether.log");

    if !log_file.exists() {
        return Ok("No logs available yet. Logs will appear here once the app starts generating them.".to_string());
    }

    let content = std::fs::read_to_string(&log_file)
        .map_err(|e| format!("Failed to read log file: {}", e))?;

    // Return last N lines if specified
    if let Some(n) = lines {
        let all_lines: Vec<&str> = content.lines().collect();
        let start = if all_lines.len() > n { all_lines.len() - n } else { 0 };
        Ok(all_lines[start..].join("\n"))
    } else {
        Ok(content)
    }
}

#[tauri::command]
async fn open_project_window(
    app: tauri::AppHandle,
    project_path: String,
) -> Result<(), String> {
    use tauri::Manager;

    log::info!("Opening new window for project: {}", project_path);

    // Create a unique window label
    let window_label = format!("project-{}", std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis());

    // Get the main window's URL
    let main_window = app.get_webview_window("main")
        .ok_or("Main window not found")?;

    let current_url = main_window.url()
        .map_err(|e| format!("Failed to get current URL: {}", e))?;

    // Create URL with project path as query parameter
    let mut url = current_url.clone();
    url.set_query(Some(&format!("project={}", urlencoding::encode(&project_path))));

    log::info!("Opening window with URL: {}", url);

    // Create new window
    tauri::WebviewWindowBuilder::new(
        &app,
        &window_label,
        tauri::WebviewUrl::External(url),
    )
    .title("tether")
    .inner_size(800.0, 600.0)
    .build()
    .map_err(|e| format!("Failed to create window: {}", e))?;

    log::info!("Window created with label: {}", window_label);
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    use tauri::menu::{MenuBuilder, MenuItemBuilder, SubmenuBuilder};

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        // NOTE: window-state plugin disabled - causes startup hangs on first launch
        // .plugin(tauri_plugin_window_state::Builder::default().build())
        .plugin(tauri_plugin_fs::init())
        .plugin(
            tauri_plugin_log::Builder::new()
                .target(tauri_plugin_log::Target::new(
                    tauri_plugin_log::TargetKind::LogDir {
                        file_name: Some("tether".to_string()),
                    },
                ))
                .level(log::LevelFilter::Info)
                .build(),
        )
        .setup(|app| {
            // Create custom menu items
            let open_new_window = MenuItemBuilder::with_id("open_new_window", "Open Project in New Window...")
                .accelerator("Cmd+Shift+O")
                .build(app)?;

            let show_logs = MenuItemBuilder::with_id("show_logs", "Show Runtime Logs")
                .accelerator("Cmd+Shift+L")
                .build(app)?;

            let open_logs_folder = MenuItemBuilder::with_id("open_logs_folder", "Open Logs Folder")
                .build(app)?;

            // Create "About" menu item for app menu
            let about_item = MenuItemBuilder::with_id("about", "About tether")
                .build(app)?;

            // Build App menu (appears as "tether" in menu bar)
            // On macOS, this MUST be the first submenu
            let app_menu = SubmenuBuilder::new(app, "tether")
                .item(&about_item)
                .separator()
                .quit()
                .build()?;

            // Create "New Workbook" menu item
            let new_workbook = MenuItemBuilder::with_id("new_workbook", "New Workbook")
                .accelerator("Cmd+N")
                .build(app)?;

            let open_project = MenuItemBuilder::with_id("open_project", "Open Project...")
                .accelerator("Cmd+O")
                .build(app)?;

            // Build File menu (now second submenu)
            let file_menu = SubmenuBuilder::new(app, "File")
                .item(&new_workbook)
                .item(&open_project)
                .separator()
                .item(&open_new_window)
                .build()?;

            log::info!("File menu created successfully");

            // Build Edit menu
            let edit_menu = SubmenuBuilder::new(app, "Edit")
                .undo()
                .redo()
                .separator()
                .cut()
                .copy()
                .paste()
                .separator()
                .select_all()
                .build()?;

            // Build View menu
            let view_menu = SubmenuBuilder::new(app, "View")
                .item(&show_logs)
                .item(&open_logs_folder)
                .build()?;

            // Build Window menu
            let window_menu = SubmenuBuilder::new(app, "Window")
                .minimize()
                .maximize()
                .separator()
                .close_window()
                .build()?;

            log::info!("Building complete menu bar");

            // Build the complete menu
            // On macOS, the app menu MUST be first, followed by File, Edit, View, Window
            let menu = MenuBuilder::new(app)
                .items(&[&app_menu, &file_menu, &edit_menu, &view_menu, &window_menu])
                .build()?;

            log::info!("Menu bar built, setting on app");
            app.set_menu(menu)?;
            log::info!("Menu bar set successfully");

            // Handle menu events
            app.on_menu_event(move |app_handle, event| {
                log::info!("Menu event triggered: {}", event.id().as_ref());
                match event.id().as_ref() {
                    "about" => {
                        if let Err(e) = app_handle.emit("menu:about", ()) {
                            log::error!("Failed to emit menu event: {}", e);
                        }
                    }
                    "new_workbook" => {
                        if let Err(e) = app_handle.emit("menu:new-workbook", ()) {
                            log::error!("Failed to emit menu event: {}", e);
                        }
                    }
                    "open_project" => {
                        if let Err(e) = app_handle.emit("menu:open-project", ()) {
                            log::error!("Failed to emit menu event: {}", e);
                        }
                    }
                    "open_new_window" => {
                        if let Err(e) = app_handle.emit("menu:open-new-window", ()) {
                            log::error!("Failed to emit menu event: {}", e);
                        }
                    }
                    "show_logs" => {
                        if let Err(e) = app_handle.emit("menu:show-logs", ()) {
                            log::error!("Failed to emit menu event: {}", e);
                        }
                    }
                    "open_logs_folder" => {
                        if let Err(e) = app_handle.emit("menu:open-logs-folder", ()) {
                            log::error!("Failed to emit menu event: {}", e);
                        }
                    }
                    _ => {}
                }
            });

            Ok(())
        })
        .manage(AppState {
            current_project: Mutex::new(None),
            engine_server: Arc::new(Mutex::new(None)),
            secrets_manager: Arc::new(Mutex::new(None)),
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
            read_file_binary,
            save_file,
            rename_file,
            delete_file,
            create_new_file,
            create_new_folder,
            get_file_info,
            reveal_in_finder,
            duplicate_workbook,
            save_dropped_file,
            save_dropped_folder,
            handle_dropped_item,
            ensure_engine_server,
            start_engine,
            execute_cell,
            execute_cell_stream,
            complete_code,
            stop_engine,
            interrupt_engine,
            restart_engine,
            add_secret,
            get_secret,
            get_secret_authenticated,
            list_secrets,
            update_secret,
            delete_secret,
            get_all_secrets,
            import_secrets_from_env,
            scan_outputs_for_secrets,
            authenticate_secrets_access,
            check_secrets_session,
            lock_secrets_session,
            test_secrets_loading,
            open_project_window,
            get_logs_directory,
            open_logs_folder,
            get_recent_logs,
            cli_install::install_cli,
            cli_install::check_cli_installed,
            cli_install::get_path_instructions,
            cli_install::get_bundled_cli_version,
            cli_install::get_installed_cli_version,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
