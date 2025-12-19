use clap::{Parser, Subcommand};
use std::path::PathBuf;
use tether_lib::scheduler::{CronPreset, SchedulerManager};
use tether_lib::{engine_http, python, project};

#[derive(Parser)]
#[command(name = "tether")]
#[command(about = "Durable workbook orchestration for local-first data pipelines", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
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
        Commands::Run { workbook, project } => {
            if let Err(e) = run_workbook(&workbook, project.as_ref()).await {
                eprintln!("Error running workbook: {}", e);
                std::process::exit(1);
            }
        }
        Commands::Schedule { action } => {
            if let Err(e) = handle_schedule_action(action).await {
                eprintln!("Error: {}", e);
                std::process::exit(1);
            }
        }
    }
}

async fn run_workbook(workbook: &PathBuf, project: Option<&PathBuf>) -> anyhow::Result<()> {
    println!("Running workbook: {}", workbook.display());

    // Determine project root
    let project_root = if let Some(p) = project {
        p.clone()
    } else {
        // Try to find project by looking for .tether directory in workbook's parent
        let workbook_parent = workbook
            .parent()
            .ok_or_else(|| anyhow::anyhow!("Could not determine workbook directory"))?;

        let mut current = workbook_parent;
        let mut found_tether = false;
        let mut result_path = workbook_parent.to_path_buf();

        loop {
            let tether_dir = current.join(".tether");
            if tether_dir.exists() {
                found_tether = true;
                result_path = current.to_path_buf();
                break;
            }

            if let Some(parent) = current.parent() {
                current = parent;
            } else {
                // No .tether found, use workbook's parent
                break;
            }
        }

        result_path
    };

    println!("Project root: {}", project_root.display());

    // Try to load the project
    let tether_project = project::load_project(&project_root)
        .unwrap_or_else(|_| {
            // If not a Tether project, create minimal project info
            let folder_name = project_root
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("project")
                .to_string();

            println!("⚠ Not a Tether project. Running in basic mode.");

            project::TetherProject {
                name: folder_name.clone(),
                package_name: folder_name.to_lowercase().replace(" ", "-"),
                root: project_root.clone(),
            }
        });

    println!("Project: {}", tether_project.name);

    // Ensure Python environment exists
    println!("Ensuring Python environment...");
    let venv_path = python::ensure_venv(&tether_project.root, &tether_project.package_name).await?;
    println!("✓ Virtual environment: {}", venv_path.display());

    // Sync dependencies if pyproject.toml exists
    if tether_project.root.join("pyproject.toml").exists() {
        println!("Syncing dependencies...");
        python::sync_dependencies(&tether_project.root, &venv_path).await?;
        println!("✓ Dependencies synced");
    }

    // Read the workbook file
    let workbook_content = std::fs::read_to_string(workbook)?;
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

    let code_cell_count = cells.iter().filter(|c| c.cell_type == "code").count();
    println!("\nFound {} code cells", code_cell_count);

    // Start engine server
    println!("Starting engine server...");
    let engine_server = engine_http::EngineServer::start().await?;

    println!("✓ Engine server started on port {}", engine_server.port);

    // Get workbook path as string
    let workbook_str = workbook.to_string_lossy().to_string();

    // Start engine for this workbook
    println!("Initializing engine...");
    engine_http::EngineServer::start_engine_http(
        engine_server.port,
        &workbook_str,
        &tether_project.root,
        &venv_path,
    ).await?;
    println!("✓ Engine initialized\n");

    // Execute all cells
    println!("Executing notebook...");
    let result = engine_http::EngineServer::execute_all_http(
        engine_server.port,
        &workbook_str,
        cells,
    ).await?;

    // Display results
    println!("\n{}", "=".repeat(60));
    if result.success {
        println!("✓ Execution completed successfully");
    } else {
        println!("✗ Execution failed");
    }
    println!("  Total cells: {}", result.total_cells);
    println!("  Successful: {}", result.successful_cells);
    println!("  Failed: {}", result.failed_cells);
    println!("{}", "=".repeat(60));

    // Show cell outputs
    for cell_result in &result.cell_results {
        println!("\nCell {}: {}",
            cell_result.cell_index + 1,
            if cell_result.success { "✓" } else { "✗" }
        );

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
                    eprintln!("\n{}: {}", ename, evalue);
                    for line in traceback {
                        eprintln!("{}", line);
                    }
                }
                _ => {}
            }
        }
    }

    // Stop engine
    engine_http::EngineServer::stop_engine_http(engine_server.port, &workbook_str).await?;
    engine_server.shutdown()?;

    if result.success {
        Ok(())
    } else {
        anyhow::bail!("Execution failed")
    }
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
