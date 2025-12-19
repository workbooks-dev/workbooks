use clap::{Parser, Subcommand};
use std::path::PathBuf;
use tether_lib::scheduler::{CronPreset, SchedulerManager};
use tether_lib::{engine_http, python, project};

#[derive(Parser)]
#[command(name = "tether")]
#[command(version)]
#[command(about = "Durable workbook orchestration for local-first data pipelines", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Run a workbook and execute all cells
    Run {
        /// Path to the workbook file
        workbook: PathBuf,

        /// Project root directory (defaults to workbook's parent)
        #[arg(short, long)]
        project: Option<PathBuf>,
    },

    /// Manage scheduled workbooks
    Schedule {
        #[command(subcommand)]
        action: ScheduleAction,
    },
}

#[derive(Subcommand)]
enum ScheduleAction {
    /// Add a new schedule for a workbook
    Add {
        /// Path to the workbook file
        workbook: PathBuf,

        /// Project root directory (defaults to workbook's parent)
        #[arg(short, long)]
        project: Option<PathBuf>,

        /// Cron expression (e.g., "0 9 * * *" for daily at 9am)
        #[arg(short, long, conflicts_with_all = ["daily", "hourly", "weekly"])]
        cron: Option<String>,

        /// Run daily at 9am
        #[arg(long, conflicts_with_all = ["cron", "hourly", "weekly"])]
        daily: bool,

        /// Run hourly
        #[arg(long, conflicts_with_all = ["cron", "daily", "weekly"])]
        hourly: bool,

        /// Run weekly on Mondays at 9am
        #[arg(long, conflicts_with_all = ["cron", "daily", "hourly"])]
        weekly: bool,
    },

    /// List all scheduled workbooks
    List,

    /// Remove a schedule
    Remove {
        /// Schedule ID to remove
        id: String,
    },
}

#[tokio::main]
async fn main() {
    // Initialize logger for CLI
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
        .init();

    let cli = Cli::parse();

    match cli.command {
        Some(Commands::Run { workbook, project }) => {
            if let Err(e) = run_workbook(&workbook, project.as_ref()).await {
                eprintln!("Error: {}", e);
                std::process::exit(1);
            }
        }
        Some(Commands::Schedule { action }) => {
            if let Err(e) = handle_schedule_action(action).await {
                eprintln!("Error: {}", e);
                std::process::exit(1);
            }
        }
        None => {
            // No subcommand provided, show help
            eprintln!("tether: Durable workbook orchestration for local-first data pipelines");
            eprintln!("\nUsage: tether <COMMAND>\n");
            eprintln!("Commands:");
            eprintln!("  run        Run a workbook and execute all cells");
            eprintln!("  schedule   Manage scheduled workbooks");
            eprintln!("\nOptions:");
            eprintln!("  -h, --help     Print help");
            eprintln!("  -V, --version  Print version");
            std::process::exit(1);
        }
    }
}

async fn run_workbook(workbook: &PathBuf, project: Option<&PathBuf>) -> anyhow::Result<()> {
    // Canonicalize workbook path to handle relative paths
    let workbook_path = std::fs::canonicalize(workbook)
        .unwrap_or_else(|_| workbook.clone());

    // Determine project root
    let project_root = if let Some(p) = project {
        std::fs::canonicalize(p)?
    } else {
        // Try to find project by looking for .tether directory
        // Start from workbook's parent, or current dir if workbook is just a filename
        let start_dir = workbook_path
            .parent()
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));

        let mut current = start_dir.as_path();
        let mut result_path = start_dir.clone();

        loop {
            let tether_dir = current.join(".tether");
            if tether_dir.exists() {
                result_path = current.to_path_buf();
                break;
            }

            if let Some(parent) = current.parent() {
                current = parent;
            } else {
                // No .tether found, use starting directory
                break;
            }
        }

        result_path
    };

    // Try to load the project
    let (tether_project, is_tether_project) = match project::load_project(&project_root) {
        Ok(project) => (project, true),
        Err(_) => {
            // If not a Tether project, create minimal project info
            let folder_name = project_root
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("project")
                .to_string();

            (
                project::TetherProject {
                    name: folder_name.clone(),
                    package_name: folder_name.to_lowercase().replace(" ", "-"),
                    root: project_root.clone(),
                },
                false
            )
        }
    };

    // Ensure Python environment exists (silent)
    let venv_path = python::ensure_venv(&tether_project.root, &tether_project.package_name).await?;

    // Sync dependencies if pyproject.toml exists (silent)
    if tether_project.root.join("pyproject.toml").exists() {
        let group = if is_tether_project { Some("tether") } else { None };
        python::sync_dependencies_with_group(&tether_project.root, &venv_path, group).await?;
    }

    // Ensure ipykernel is installed (silent)
    python::ensure_ipykernel(&venv_path).await?;

    // Read the workbook file
    let workbook_content = std::fs::read_to_string(&workbook_path)?;
    let notebook: serde_json::Value = serde_json::from_str(&workbook_content)?;

    // Extract cells
    let cells_json = notebook["cells"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("Invalid notebook format: missing cells"))?;

    // Convert to Cell structs
    let cells: Vec<engine_http::Cell> = cells_json
        .iter()
        .map(|cell| {
            let cell_type = cell["cell_type"]
                .as_str()
                .unwrap_or("code")
                .to_string();

            let source = cell["source"]
                .as_array()
                .map(|arr| {
                    arr.iter()
                        .filter_map(|v| v.as_str())
                        .collect::<Vec<_>>()
                        .join("")
                })
                .or_else(|| cell["source"].as_str().map(String::from))
                .unwrap_or_default();

            engine_http::Cell {
                source,
                cell_type,
            }
        })
        .collect();

    // Start engine server (silent)
    let engine_server = engine_http::EngineServer::start().await?;

    // Get workbook path as string
    let workbook_str = workbook_path.to_string_lossy().to_string();

    // Start engine for this workbook (silent)
    engine_http::EngineServer::start_engine_http(
        engine_server.port,
        &workbook_str,
        &tether_project.root,
        &venv_path,
    ).await?;

    // Execute all cells
    let result = engine_http::EngineServer::execute_all_http(
        engine_server.port,
        &workbook_str,
        cells,
    ).await?;

    // Show only cell outputs
    for cell_result in &result.cell_results {
        // Show outputs
        for output in &cell_result.outputs {
            match output {
                engine_http::CellOutput::Stream { name, text } => {
                    if name == "stdout" {
                        print!("{}", text);
                    } else {
                        eprint!("{}", text);
                    }
                }
                engine_http::CellOutput::ExecuteResult { data, .. } => {
                    if let Some(text) = data.get("text/plain") {
                        println!("{}", text);
                    }
                }
                engine_http::CellOutput::Error { ename, evalue, traceback } => {
                    eprintln!("Error: {}: {}", ename, evalue);
                    for line in traceback {
                        eprintln!("{}", line);
                    }
                }
                _ => {}
            }
        }
    }

    // Stop engine (silent)
    engine_http::EngineServer::stop_engine_http(engine_server.port, &workbook_str).await?;
    engine_server.shutdown()?;

    if !result.success {
        std::process::exit(1);
    }

    Ok(())
}

async fn handle_schedule_action(action: ScheduleAction) -> anyhow::Result<()> {
    let manager = SchedulerManager::new()?;

    match action {
        ScheduleAction::Add {
            workbook,
            project,
            cron,
            daily,
            hourly,
            weekly,
        } => {
            // Determine cron expression
            let cron_expr = if let Some(expr) = cron {
                expr
            } else if daily {
                CronPreset::Daily.to_cron_expression().to_string()
            } else if hourly {
                CronPreset::Hourly.to_cron_expression().to_string()
            } else if weekly {
                CronPreset::Weekly.to_cron_expression().to_string()
            } else {
                return Err(anyhow::anyhow!(
                    "Please specify a schedule: --cron, --daily, --hourly, or --weekly"
                ));
            };

            // Determine project root
            let project_root = if let Some(p) = project {
                p
            } else {
                workbook
                    .parent()
                    .ok_or_else(|| anyhow::anyhow!("Could not determine project root"))?
                    .to_path_buf()
            };

            // Canonicalize paths
            let workbook_abs = std::fs::canonicalize(&workbook)?;
            let project_abs = std::fs::canonicalize(&project_root)?;

            let schedule = manager.add_schedule(
                workbook_abs.to_str().unwrap(),
                project_abs.to_str().unwrap(),
                &cron_expr,
            )?;

            println!("✓ Schedule added successfully!");
            println!("  ID: {}", schedule.id);
            println!("  Workbook: {}", schedule.workbook_path);
            println!("  Cron: {}", schedule.cron_expression);
            println!("\nNote: Schedules run when the Tether GUI app is open.");
        }

        ScheduleAction::List => {
            let schedules = manager.list_schedules()?;

            if schedules.is_empty() {
                println!("No schedules found.");
                return Ok(());
            }

            println!("Scheduled Workbooks:\n");
            for schedule in schedules {
                println!("ID: {}", schedule.id);
                println!("  Workbook: {}", schedule.workbook_path);
                println!("  Project: {}", schedule.project_root);
                println!("  Cron: {}", schedule.cron_expression);
                println!("  Enabled: {}", if schedule.enabled { "yes" } else { "no" });
                if let Some(next_run) = schedule.next_run {
                    let dt = chrono::DateTime::from_timestamp(next_run, 0)
                        .unwrap_or_default();
                    println!("  Next run: {}", dt);
                }
                println!();
            }
        }

        ScheduleAction::Remove { id } => {
            manager.delete_schedule(&id)?;
            println!("✓ Schedule removed successfully!");
        }
    }

    Ok(())
}
