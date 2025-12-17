mod python;
mod project;
mod fs;
mod kernel_http;

use std::path::PathBuf;
use tauri::State;
use std::sync::Arc;
use tokio::sync::Mutex;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct ProjectInfo {
    pub name: String,
    pub root: String,
}

/// Application state to track the current project and kernel server
pub struct AppState {
    pub project_root: Mutex<Option<PathBuf>>,
    pub kernel_server: Arc<Mutex<Option<kernel_http::KernelServer>>>,
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
async fn init_python_env(project_path: String) -> Result<String, String> {
    let path = PathBuf::from(project_path);

    python::init_project(&path)
        .await
        .map_err(|e| e.to_string())?;

    Ok("Python environment initialized successfully".to_string())
}

#[tauri::command]
async fn ensure_python_venv(project_path: String) -> Result<String, String> {
    let path = PathBuf::from(project_path);

    let venv_path = python::ensure_venv(&path)
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
async fn run_python_code(project_path: String, code: String) -> Result<String, String> {
    let path = PathBuf::from(project_path);

    let output = python::run_python_command(&path, &["-c", &code])
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

    // Set as current project
    let mut root = state.project_root.lock().await;
    *root = Some(project.root.clone());

    Ok(ProjectInfo {
        name: project.name,
        root: project.root.to_string_lossy().to_string(),
    })
}

#[tauri::command]
async fn open_folder(folder_path: String, state: State<'_, AppState>) -> Result<ProjectInfo, String> {
    let path = PathBuf::from(&folder_path);

    let project = project::open_folder(&path)
        .await
        .map_err(|e| e.to_string())?;

    // Set as current project
    let mut root = state.project_root.lock().await;
    *root = Some(project.root.clone());

    Ok(ProjectInfo {
        name: project.name,
        root: project.root.to_string_lossy().to_string(),
    })
}

#[tauri::command]
async fn load_project(project_path: String, state: State<'_, AppState>) -> Result<ProjectInfo, String> {
    let path = PathBuf::from(&project_path);

    let project = project::load_project(&path)
        .map_err(|e| e.to_string())?;

    // Set as current project
    let mut root = state.project_root.lock().await;
    *root = Some(project.root.clone());

    Ok(ProjectInfo {
        name: project.name,
        root: project.root.to_string_lossy().to_string(),
    })
}

#[tauri::command]
async fn get_current_project(state: State<'_, AppState>) -> Result<Option<ProjectInfo>, String> {
    let root = state.project_root.lock().await;

    if let Some(project_root) = root.as_ref() {
        let project = project::load_project(project_root)
            .map_err(|e| e.to_string())?;

        Ok(Some(ProjectInfo {
            name: project.name,
            root: project.root.to_string_lossy().to_string(),
        }))
    } else {
        Ok(None)
    }
}

#[tauri::command]
async fn set_project_root(project_path: String, state: State<'_, AppState>) -> Result<(), String> {
    let mut root = state.project_root.lock().await;
    *root = Some(PathBuf::from(project_path));
    Ok(())
}

#[tauri::command]
async fn get_project_root(state: State<'_, AppState>) -> Result<Option<String>, String> {
    let root = state.project_root.lock().await;
    Ok(root.as_ref().map(|p| p.to_string_lossy().to_string()))
}

#[tauri::command]
async fn list_files(directory_path: String) -> Result<Vec<fs::FileEntry>, String> {
    let path = PathBuf::from(directory_path);
    fs::list_directory(&path).map_err(|e| e.to_string())
}

#[tauri::command]
async fn create_notebook(
    notebook_path: String,
    notebook_name: String,
) -> Result<String, String> {
    let path = PathBuf::from(notebook_path);
    fs::create_notebook(&path, &notebook_name).map_err(|e| e.to_string())
}

#[tauri::command]
async fn read_notebook(notebook_path: String) -> Result<String, String> {
    let path = PathBuf::from(notebook_path);
    fs::read_notebook(&path).map_err(|e| e.to_string())
}

#[tauri::command]
async fn save_notebook(
    notebook_path: String,
    content: String,
) -> Result<(), String> {
    let path = PathBuf::from(notebook_path);
    fs::save_notebook(&path, &content).map_err(|e| e.to_string())
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
async fn ensure_kernel_server(state: State<'_, AppState>) -> Result<(), String> {
    // Use async mutex to hold lock across await - prevents race condition
    let mut server = state.kernel_server.lock().await;

    if server.is_none() {
        println!("Starting kernel server...");
        let ks = kernel_http::KernelServer::start()
            .await
            .map_err(|e| e.to_string())?;

        *server = Some(ks);
        println!("Kernel server started successfully");
    }

    Ok(())
}

#[tauri::command]
async fn start_kernel(
    notebook_path: String,
    project_path: String,
    _kernel_name: Option<String>,
    state: State<'_, AppState>,
) -> Result<(), String> {
    // Ensure kernel server is running
    ensure_kernel_server(state.clone()).await?;

    let project_root = PathBuf::from(project_path);

    // Get the port
    let port = {
        let server = state.kernel_server.lock().await;
        server.as_ref().map(|s| s.port).ok_or("Kernel server not initialized")?
    };

    // Now call the static method without holding any locks
    kernel_http::KernelServer::start_kernel_http(port, &notebook_path, &project_root)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn execute_cell(
    notebook_path: String,
    code: String,
    state: State<'_, AppState>,
) -> Result<kernel_http::ExecutionResult, String> {
    let port = {
        let server = state.kernel_server.lock().await;
        server.as_ref().map(|s| s.port).ok_or("Kernel server not initialized")?
    };

    kernel_http::KernelServer::execute_http(port, &notebook_path, &code)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn stop_kernel(
    notebook_path: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let port = {
        let server = state.kernel_server.lock().await;
        server.as_ref().map(|s| s.port).ok_or("Kernel server not initialized")?
    };

    kernel_http::KernelServer::stop_kernel_http(port, &notebook_path)
        .await
        .map_err(|e| e.to_string())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .manage(AppState {
            project_root: Mutex::new(None),
            kernel_server: Arc::new(Mutex::new(None)),
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
            create_notebook,
            read_notebook,
            save_notebook,
            read_file,
            save_file,
            ensure_kernel_server,
            start_kernel,
            execute_cell,
            stop_kernel,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
