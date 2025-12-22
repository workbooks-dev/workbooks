pub mod python;
pub mod project;
pub mod config;
mod fs;
pub mod engine_http;
mod secrets;
pub mod scheduler;
pub mod cli_install;
mod recent_projects;
pub mod app_credentials;
pub mod global_config;
mod chat_sessions;
mod agent;

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
    pub scheduler_manager: Arc<Mutex<Option<scheduler::SchedulerManager>>>,
    pub active_agent_requests: agent::ActiveRequests,
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

    // Add to recent projects
    let _ = recent_projects::add_recent_project(&project.name, &project.root);

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

    // Add to recent projects
    let _ = recent_projects::add_recent_project(&project.name, &project.root);

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

    // Add to recent projects
    let _ = recent_projects::add_recent_project(&project.name, &project.root);

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
async fn get_recent_projects() -> Result<Vec<recent_projects::RecentProject>, String> {
    Ok(recent_projects::get_recent_projects())
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

    // Check if server exists and is healthy
    let needs_restart = if let Some(ref existing_server) = *server {
        // Try health check
        let health_url = format!("http://127.0.0.1:{}/health", existing_server.port);
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(2))
            .build()
            .map_err(|e| e.to_string())?;

        match client.get(&health_url).send().await {
            Ok(response) if response.status().is_success() => {
                log::info!("Engine server is healthy on port {}", existing_server.port);
                false // Server is healthy, no restart needed
            }
            Ok(response) => {
                log::warn!("Engine server returned status: {}, restarting...", response.status());
                true // Unhealthy, needs restart
            }
            Err(e) => {
                log::warn!("Engine server health check failed: {}, restarting...", e);
                true // Unhealthy, needs restart
            }
        }
    } else {
        true // No server, needs start
    };

    if needs_restart {
        // Drop old server if it exists
        if server.is_some() {
            log::info!("Dropping old engine server...");
            *server = None;
        }

        log::info!("Starting engine server...");
        let es = engine_http::EngineServer::start()
            .await
            .map_err(|e| {
                log::error!("Failed to start engine server: {}", e);
                e.to_string()
            })?;

        *server = Some(es);
        log::info!("Engine server started successfully on port {}", server.as_ref().unwrap().port);
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

// ==================== SCHEDULER COMMANDS ====================

/// Ensure scheduler manager is initialized
async fn ensure_scheduler_manager(state: &State<'_, AppState>) -> Result<(), String> {
    let mut manager_lock = state.scheduler_manager.lock().await;
    if manager_lock.is_none() {
        let mut new_manager = scheduler::SchedulerManager::new()
            .map_err(|e| format!("Failed to create scheduler manager: {}", e))?;

        // Start the scheduler
        new_manager.start_scheduler().await
            .map_err(|e| format!("Failed to start scheduler: {}", e))?;

        *manager_lock = Some(new_manager);
    }
    Ok(())
}

#[tauri::command]
async fn add_schedule(
    project_root: String,
    workbook_path: String,
    cron_expression: String,
    state: State<'_, AppState>,
) -> Result<scheduler::Schedule, String> {
    ensure_scheduler_manager(&state).await?;

    let manager_lock = state.scheduler_manager.lock().await;
    let manager = manager_lock.as_ref().ok_or("Scheduler manager not initialized")?;

    let schedule = manager.add_schedule(&workbook_path, &project_root, &cron_expression)
        .await
        .map_err(|e| format!("Failed to add schedule: {}", e))?;

    Ok(schedule)
}

#[tauri::command]
async fn list_schedules(
    state: State<'_, AppState>,
) -> Result<Vec<scheduler::Schedule>, String> {
    ensure_scheduler_manager(&state).await?;

    let manager_lock = state.scheduler_manager.lock().await;
    let manager = manager_lock.as_ref().ok_or("Scheduler manager not initialized")?;

    manager.list_schedules()
        .map_err(|e| format!("Failed to list schedules: {}", e))
}

#[tauri::command]
async fn update_schedule(
    schedule_id: String,
    cron_expression: Option<String>,
    enabled: Option<bool>,
    state: State<'_, AppState>,
) -> Result<(), String> {
    ensure_scheduler_manager(&state).await?;

    let manager_lock = state.scheduler_manager.lock().await;
    let manager = manager_lock.as_ref().ok_or("Scheduler manager not initialized")?;

    manager.update_schedule(&schedule_id, cron_expression.as_deref(), enabled)
        .await
        .map_err(|e| format!("Failed to update schedule: {}", e))
}

#[tauri::command]
async fn delete_schedule(
    schedule_id: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    ensure_scheduler_manager(&state).await?;

    let manager_lock = state.scheduler_manager.lock().await;
    let manager = manager_lock.as_ref().ok_or("Scheduler manager not initialized")?;

    manager.delete_schedule(&schedule_id)
        .await
        .map_err(|e| format!("Failed to delete schedule: {}", e))
}

#[tauri::command]
async fn list_runs(
    limit: usize,
    state: State<'_, AppState>,
) -> Result<Vec<scheduler::Run>, String> {
    ensure_scheduler_manager(&state).await?;

    let manager_lock = state.scheduler_manager.lock().await;
    let manager = manager_lock.as_ref().ok_or("Scheduler manager not initialized")?;

    manager.list_runs(limit)
        .map_err(|e| format!("Failed to list runs: {}", e))
}

#[tauri::command]
async fn list_runs_paginated(
    limit: usize,
    offset: usize,
    start_time: Option<i64>,
    end_time: Option<i64>,
    state: State<'_, AppState>,
) -> Result<Vec<scheduler::Run>, String> {
    ensure_scheduler_manager(&state).await?;

    let manager_lock = state.scheduler_manager.lock().await;
    let manager = manager_lock.as_ref().ok_or("Scheduler manager not initialized")?;

    manager.list_runs_paginated(limit, offset, start_time, end_time)
        .map_err(|e| format!("Failed to list runs: {}", e))
}

#[tauri::command]
async fn count_runs(
    start_time: Option<i64>,
    end_time: Option<i64>,
    state: State<'_, AppState>,
) -> Result<usize, String> {
    ensure_scheduler_manager(&state).await?;

    let manager_lock = state.scheduler_manager.lock().await;
    let manager = manager_lock.as_ref().ok_or("Scheduler manager not initialized")?;

    manager.count_runs(start_time, end_time)
        .map_err(|e| format!("Failed to count runs: {}", e))
}

#[tauri::command]
async fn run_schedule_now(
    schedule_id: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    ensure_scheduler_manager(&state).await?;

    let manager_lock = state.scheduler_manager.lock().await;
    let manager = manager_lock.as_ref().ok_or("Scheduler manager not initialized")?;

    manager.run_now(&schedule_id)
        .await
        .map_err(|e| format!("Failed to run schedule: {}", e))
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
    use tauri::tray::TrayIconBuilder;

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_screenshots::init())
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
            // Create system tray with recent projects
            let mut tray_items: Vec<Box<dyn tauri::menu::IsMenuItem<tauri::Wry>>> = Vec::new();

            // Add recent projects (max 3)
            let recent_projects = recent_projects::get_recent_projects();
            if !recent_projects.is_empty() {
                for (i, project) in recent_projects.iter().enumerate() {
                    let item = MenuItemBuilder::with_id(
                        format!("tray_recent_{}", i),
                        &project.name
                    ).build(app)?;
                    tray_items.push(Box::new(item));
                }
                tray_items.push(Box::new(tauri::menu::PredefinedMenuItem::separator(app)?));
            }

            // Project management items
            let create_project_item = MenuItemBuilder::with_id("tray_create_project", "Create Project...")
                .build(app)?;
            tray_items.push(Box::new(create_project_item));

            let open_project_item = MenuItemBuilder::with_id("tray_open_project", "Open Project...")
                .build(app)?;
            tray_items.push(Box::new(open_project_item));

            tray_items.push(Box::new(tauri::menu::PredefinedMenuItem::separator(app)?));

            // Navigation items
            let view_runs_item = MenuItemBuilder::with_id("tray_view_runs", "View Runs")
                .build(app)?;
            tray_items.push(Box::new(view_runs_item));

            let view_scheduler_item = MenuItemBuilder::with_id("tray_view_scheduler", "View Scheduler")
                .build(app)?;
            tray_items.push(Box::new(view_scheduler_item));

            tray_items.push(Box::new(tauri::menu::PredefinedMenuItem::separator(app)?));

            // MCP management
            let install_mcp_item = MenuItemBuilder::with_id("tray_install_mcp", "Install MCP...")
                .build(app)?;
            tray_items.push(Box::new(install_mcp_item));

            tray_items.push(Box::new(tauri::menu::PredefinedMenuItem::separator(app)?));

            // Settings
            let settings_item = MenuItemBuilder::with_id("tray_settings", "Settings...")
                .build(app)?;
            tray_items.push(Box::new(settings_item));

            tray_items.push(Box::new(tauri::menu::PredefinedMenuItem::separator(app)?));

            // Status and quit
            let scheduler_status_item = MenuItemBuilder::with_id("tray_scheduler_status", "Scheduler: Running")
                .enabled(false)
                .build(app)?;
            tray_items.push(Box::new(scheduler_status_item));

            let quit_item = MenuItemBuilder::with_id("tray_quit", "Quit Tether")
                .build(app)?;
            tray_items.push(Box::new(quit_item));

            // Build menu from items
            let tray_menu = tauri::menu::MenuBuilder::new(app);
            let mut tray_menu = tray_menu;
            for item in tray_items {
                tray_menu = tray_menu.item(&*item);
            }
            let tray_menu = tray_menu.build()?;

            let _tray = TrayIconBuilder::new()
                .menu(&tray_menu)
                .icon(app.default_window_icon().unwrap().clone())
                // Note: Tray icon click handler removed because on macOS, clicking the icon
                // to open the menu fires a Click event, which would reset the window.
                // All functionality is accessible through menu items instead.
                .on_menu_event(|app, event| {
                    let event_id = event.id().as_ref();

                    // Helper function to show main window (creates new window if none exist)
                    // Returns true if a window was shown/created
                    let show_main_window = || -> bool {
                        // Check if any windows exist
                        let windows = app.webview_windows();
                        let window_labels: Vec<String> = windows.keys().cloned().collect();
                        log::info!("Available windows: {:?}", window_labels);

                        // If main window exists, show and focus it
                        if let Some(main_window) = windows.get("main") {
                            log::info!("Found main window, showing it");
                            let _ = main_window.show();
                            let _ = main_window.set_focus();
                            true
                        } else if !windows.is_empty() {
                            // Some other window exists, focus the first one
                            log::info!("Main window not found, focusing first available window");
                            if let Some((_, window)) = windows.iter().next() {
                                let _ = window.show();
                                let _ = window.set_focus();
                                true
                            } else {
                                false
                            }
                        } else {
                            // No windows exist - don't create one here, let the caller handle it
                            false
                        }
                    };

                    // Helper to create a new main window with optional view parameter
                    let create_main_window = |view: Option<&str>| -> bool {
                        log::info!("Creating new main window with view: {:?}", view);

                        // Build URL with optional view parameter
                        let url = if let Some(v) = view {
                            format!("index.html?view={}", v)
                        } else {
                            "index.html".to_string()
                        };

                        match tauri::WebviewWindowBuilder::new(
                            app,
                            "main",
                            tauri::WebviewUrl::App(url.into())
                        )
                        .title("tether")
                        .inner_size(1200.0, 800.0)
                        .build() {
                            Ok(window) => {
                                // Add close handler to hide instead of quit
                                let window_clone = window.clone();
                                window.on_window_event(move |event| {
                                    if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                                        let _ = window_clone.hide();
                                        api.prevent_close();
                                        log::info!("Window hidden - app continues running in background");
                                    }
                                });

                                let _ = window.show();
                                let _ = window.set_focus();
                                log::info!("Created and showed new main window");
                                true
                            }
                            Err(e) => {
                                log::error!("Failed to create new window: {}", e);
                                false
                            }
                        }
                    };

                    // Handle recent project clicks
                    if event_id.starts_with("tray_recent_") {
                        if let Some(index_str) = event_id.strip_prefix("tray_recent_") {
                            if let Ok(index) = index_str.parse::<usize>() {
                                let recent_projects = recent_projects::get_recent_projects();
                                if let Some(project) = recent_projects.get(index) {
                                    let project_path = project.path.clone();
                                    let project_name = project.name.clone();

                                    if !show_main_window() {
                                        // No window exists - create one with project parameter
                                        // Use special format to pass project path
                                        log::info!("Creating window with project: {}", project_path.display());
                                        match tauri::WebviewWindowBuilder::new(
                                            app,
                                            "main",
                                            tauri::WebviewUrl::App(format!("index.html?project={}", urlencoding::encode(&project_path.to_string_lossy())).into())
                                        )
                                        .title("tether")
                                        .inner_size(1200.0, 800.0)
                                        .build() {
                                            Ok(window) => {
                                                // Add close handler
                                                let window_clone = window.clone();
                                                window.on_window_event(move |event| {
                                                    if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                                                        let _ = window_clone.hide();
                                                        api.prevent_close();
                                                        log::info!("Window hidden - app continues running in background");
                                                    }
                                                });
                                                let _ = window.show();
                                                let _ = window.set_focus();
                                            }
                                            Err(e) => {
                                                log::error!("Failed to create window: {}", e);
                                            }
                                        }
                                    } else {
                                        // Window exists - emit event
                                        let app_handle = app.clone();
                                        tauri::async_runtime::spawn(async move {
                                            tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
                                            let _ = app_handle.emit("open-project", serde_json::json!({
                                                "path": project_path,
                                                "name": project_name,
                                            }));
                                        });
                                    }
                                }
                            }
                        }
                        return;
                    }

                    match event_id {
                        "tray_create_project" => {
                            if !show_main_window() {
                                create_main_window(Some("create"));
                            } else {
                                let app_handle = app.clone();
                                tauri::async_runtime::spawn(async move {
                                    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
                                    let _ = app_handle.emit("tray-create-project", ());
                                });
                            }
                        }
                        "tray_open_project" => {
                            if !show_main_window() {
                                create_main_window(Some("action"));
                            } else {
                                let app_handle = app.clone();
                                tauri::async_runtime::spawn(async move {
                                    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
                                    let _ = app_handle.emit("tray-open-project", ());
                                });
                            }
                        }
                        "tray_view_runs" => {
                            if !show_main_window() {
                                create_main_window(Some("global-runs"));
                            } else {
                                let app_handle = app.clone();
                                tauri::async_runtime::spawn(async move {
                                    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
                                    let _ = app_handle.emit("tray-view-runs", ());
                                });
                            }
                        }
                        "tray_view_scheduler" => {
                            if !show_main_window() {
                                create_main_window(Some("global-schedules"));
                            } else {
                                let app_handle = app.clone();
                                tauri::async_runtime::spawn(async move {
                                    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
                                    let _ = app_handle.emit("tray-view-scheduler", ());
                                });
                            }
                        }
                        "tray_install_mcp" => {
                            if !show_main_window() {
                                create_main_window(Some("action"));
                            } else {
                                let app_handle = app.clone();
                                tauri::async_runtime::spawn(async move {
                                    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
                                    let _ = app_handle.emit("tray-install-mcp", ());
                                });
                            }
                        }
                        "tray_settings" => {
                            if !show_main_window() {
                                create_main_window(Some("settings"));
                            } else {
                                let app_handle = app.clone();
                                tauri::async_runtime::spawn(async move {
                                    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
                                    let _ = app_handle.emit("tray-settings", ());
                                });
                            }
                        }
                        "tray_quit" => {
                            // Force quit the application
                            std::process::exit(0);
                        }
                        _ => {}
                    }
                })
                .build(app)?;

            log::info!("System tray created successfully");

            // Configure window close behavior to hide instead of quit
            if let Some(main_window) = app.get_webview_window("main") {
                let window_clone = main_window.clone();
                main_window.on_window_event(move |event| {
                    if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                        // Hide the window instead of closing the app
                        let _ = window_clone.hide();
                        api.prevent_close();
                        log::info!("Window hidden - app continues running in background");
                    }
                });
                log::info!("Window close handler configured");
            }

            // Create custom menu items
            let open_new_window = MenuItemBuilder::with_id("open_new_window", "Open Project in New Window...")
                .accelerator("Cmd+Shift+O")
                .build(app)?;

            let show_logs = MenuItemBuilder::with_id("show_logs", "Show Runtime Logs")
                .accelerator("Cmd+Shift+L")
                .build(app)?;

            let open_logs_folder = MenuItemBuilder::with_id("open_logs_folder", "Open Logs Folder")
                .build(app)?;

            let take_screenshot = MenuItemBuilder::with_id("take_screenshot", "Take Screenshot")
                .accelerator("Cmd+Shift+S")
                .build(app)?;

            // Create "About" menu item for app menu
            let about_item = MenuItemBuilder::with_id("about", "About tether")
                .build(app)?;

            // Create "Settings" menu item for app menu
            let settings_item = MenuItemBuilder::with_id("menu_settings", "Settings...")
                .accelerator("Cmd+,")
                .build(app)?;

            // Build App menu (appears as "tether" in menu bar)
            // On macOS, this MUST be the first submenu
            let app_menu = SubmenuBuilder::new(app, "tether")
                .item(&about_item)
                .separator()
                .item(&settings_item)
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
                .separator()
                .item(&take_screenshot)
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
                    "menu_settings" => {
                        if let Err(e) = app_handle.emit("menu:settings", ()) {
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
                    "take_screenshot" => {
                        if let Err(e) = app_handle.emit("menu:take-screenshot", ()) {
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
            scheduler_manager: Arc::new(Mutex::new(None)),
            active_agent_requests: Arc::new(Mutex::new(std::collections::HashMap::new())),
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
            get_recent_projects,
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
            add_schedule,
            list_schedules,
            update_schedule,
            delete_schedule,
            list_runs,
            list_runs_paginated,
            count_runs,
            run_schedule_now,
            open_project_window,
            get_logs_directory,
            open_logs_folder,
            get_recent_logs,
            cli_install::install_cli,
            cli_install::check_cli_installed,
            cli_install::get_path_instructions,
            cli_install::get_bundled_cli_version,
            cli_install::get_installed_cli_version,
            app_credentials::save_anthropic_api_key,
            app_credentials::load_anthropic_api_key,
            app_credentials::remove_anthropic_api_key,
            app_credentials::check_anthropic_api_key,
            app_credentials::get_anthropic_api_key_authenticated,
            app_credentials::verify_anthropic_api_key,
            global_config::get_global_config,
            global_config::update_global_config,
            global_config::set_ai_features_enabled,
            global_config::set_default_project_path,
            global_config::add_project_to_recent,
            global_config::get_default_project,
            global_config::get_global_recent_projects,
            chat_sessions::create_chat_session,
            chat_sessions::list_chat_sessions,
            chat_sessions::get_chat_session,
            chat_sessions::delete_chat_session,
            chat_sessions::add_message_to_session,
            agent::send_agent_message,
            agent::cancel_agent_request,
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|_app_handle, event| {
            match event {
                tauri::RunEvent::ExitRequested { api, .. } => {
                    // Prevent app from quitting when windows are closed
                    // Only allow quit from tray menu
                    api.prevent_exit();
                }
                _ => {}
            }
        });
}
