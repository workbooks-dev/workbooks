use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio_cron_scheduler::{Job, JobScheduler};
use uuid::Uuid;

/// A scheduled workbook
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Schedule {
    pub id: String,
    pub workbook_path: String,
    pub project_root: String,
    pub cron_expression: String,
    pub enabled: bool,
    pub created_at: i64,
    pub modified_at: i64,
    pub next_run: Option<i64>,
    pub last_run: Option<i64>,
}

/// A workbook execution run
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Run {
    pub id: String,
    pub schedule_id: Option<String>,
    pub workbook_path: String,
    pub project_root: String,
    pub started_at: i64,
    pub finished_at: Option<i64>,
    pub duration: Option<i64>,
    pub status: String, // "success", "failed", "interrupted"
    pub error_message: Option<String>,
    pub report_path: Option<String>,
}

/// Cron presets for common scheduling patterns
pub enum CronPreset {
    Daily,
    Hourly,
    Weekly,
}

impl CronPreset {
    pub fn to_cron_expression(&self) -> &'static str {
        match self {
            CronPreset::Daily => "0 9 * * *",      // 9am daily
            CronPreset::Hourly => "0 * * * *",     // Top of every hour
            CronPreset::Weekly => "0 9 * * 1",     // 9am every Monday
        }
    }
}

/// Manager for schedules and runs
pub struct SchedulerManager {
    db_path: PathBuf,
    scheduler: Option<Arc<JobScheduler>>,
}

impl SchedulerManager {
    /// Create a new scheduler manager with global database
    pub fn new() -> Result<Self> {
        let db_path = Self::get_global_db_path()?;

        // Ensure parent directory exists
        if let Some(parent) = db_path.parent() {
            std::fs::create_dir_all(parent)
                .context("Failed to create scheduler directory")?;
        }

        let manager = Self {
            db_path,
            scheduler: None,
        };

        // Initialize database
        manager.init_db()?;

        Ok(manager)
    }

    /// Get the global scheduler database path
    fn get_global_db_path() -> Result<PathBuf> {
        let home = dirs::home_dir()
            .context("Could not find home directory")?;
        Ok(home.join(".tether").join("schedules.db"))
    }

    /// Initialize the database schema
    fn init_db(&self) -> Result<()> {
        let conn = Connection::open(&self.db_path)
            .context("Failed to open scheduler database")?;

        conn.execute(
            "CREATE TABLE IF NOT EXISTS schedules (
                id TEXT PRIMARY KEY,
                workbook_path TEXT NOT NULL,
                project_root TEXT NOT NULL,
                cron_expression TEXT NOT NULL,
                enabled INTEGER NOT NULL DEFAULT 1,
                created_at INTEGER NOT NULL,
                modified_at INTEGER NOT NULL,
                next_run INTEGER,
                last_run INTEGER,
                UNIQUE(workbook_path, project_root)
            )",
            [],
        )?;

        conn.execute(
            "CREATE TABLE IF NOT EXISTS runs (
                id TEXT PRIMARY KEY,
                schedule_id TEXT,
                workbook_path TEXT NOT NULL,
                project_root TEXT NOT NULL,
                started_at INTEGER NOT NULL,
                finished_at INTEGER,
                duration INTEGER,
                status TEXT NOT NULL,
                error_message TEXT,
                report_path TEXT,
                FOREIGN KEY (schedule_id) REFERENCES schedules(id)
            )",
            [],
        )?;

        // Create index on runs for faster queries
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_runs_started_at ON runs(started_at DESC)",
            [],
        )?;

        Ok(())
    }

    fn get_connection(&self) -> Result<Connection> {
        Connection::open(&self.db_path)
            .context("Failed to open scheduler database")
    }

    /// Add a new schedule
    pub fn add_schedule(
        &self,
        workbook_path: &str,
        project_root: &str,
        cron_expression: &str,
    ) -> Result<Schedule> {
        // Validate cron expression
        self.validate_cron(cron_expression)?;

        let now = Utc::now().timestamp();
        let id = Uuid::new_v4().to_string();

        let schedule = Schedule {
            id: id.clone(),
            workbook_path: workbook_path.to_string(),
            project_root: project_root.to_string(),
            cron_expression: cron_expression.to_string(),
            enabled: true,
            created_at: now,
            modified_at: now,
            next_run: self.calculate_next_run(cron_expression)?,
            last_run: None,
        };

        let conn = self.get_connection()?;
        conn.execute(
            "INSERT INTO schedules (id, workbook_path, project_root, cron_expression, enabled, created_at, modified_at, next_run, last_run)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
             ON CONFLICT(workbook_path, project_root) DO UPDATE SET
                cron_expression = excluded.cron_expression,
                modified_at = excluded.modified_at,
                next_run = excluded.next_run,
                enabled = excluded.enabled",
            params![
                schedule.id,
                schedule.workbook_path,
                schedule.project_root,
                schedule.cron_expression,
                schedule.enabled as i32,
                schedule.created_at,
                schedule.modified_at,
                schedule.next_run,
                schedule.last_run,
            ],
        )?;

        Ok(schedule)
    }

    /// List all schedules
    pub fn list_schedules(&self) -> Result<Vec<Schedule>> {
        let conn = self.get_connection()?;
        let mut stmt = conn.prepare(
            "SELECT id, workbook_path, project_root, cron_expression, enabled, created_at, modified_at, next_run, last_run
             FROM schedules
             ORDER BY created_at DESC"
        )?;

        let schedules = stmt.query_map([], |row| {
            Ok(Schedule {
                id: row.get(0)?,
                workbook_path: row.get(1)?,
                project_root: row.get(2)?,
                cron_expression: row.get(3)?,
                enabled: row.get::<_, i32>(4)? == 1,
                created_at: row.get(5)?,
                modified_at: row.get(6)?,
                next_run: row.get(7)?,
                last_run: row.get(8)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;

        Ok(schedules)
    }

    /// Get a schedule by ID
    pub fn get_schedule(&self, id: &str) -> Result<Option<Schedule>> {
        let conn = self.get_connection()?;
        let mut stmt = conn.prepare(
            "SELECT id, workbook_path, project_root, cron_expression, enabled, created_at, modified_at, next_run, last_run
             FROM schedules
             WHERE id = ?1"
        )?;

        let schedule = stmt.query_row([id], |row| {
            Ok(Schedule {
                id: row.get(0)?,
                workbook_path: row.get(1)?,
                project_root: row.get(2)?,
                cron_expression: row.get(3)?,
                enabled: row.get::<_, i32>(4)? == 1,
                created_at: row.get(5)?,
                modified_at: row.get(6)?,
                next_run: row.get(7)?,
                last_run: row.get(8)?,
            })
        }).optional()?;

        Ok(schedule)
    }

    /// Update a schedule
    pub fn update_schedule(
        &self,
        id: &str,
        cron_expression: Option<&str>,
        enabled: Option<bool>,
    ) -> Result<()> {
        let conn = self.get_connection()?;
        let now = Utc::now().timestamp();

        // If cron expression is being updated, validate it
        if let Some(cron) = cron_expression {
            self.validate_cron(cron)?;
        }

        // Build dynamic update query
        let mut updates = vec!["modified_at = ?1"];
        let mut params_vec: Vec<Box<dyn rusqlite::ToSql>> = vec![Box::new(now)];

        if let Some(cron) = cron_expression {
            updates.push("cron_expression = ?");
            params_vec.push(Box::new(cron.to_string()));

            // Recalculate next run
            if let Some(next_run) = self.calculate_next_run(cron)? {
                updates.push("next_run = ?");
                params_vec.push(Box::new(next_run));
            }
        }

        if let Some(en) = enabled {
            updates.push("enabled = ?");
            params_vec.push(Box::new(en as i32));
        }

        params_vec.push(Box::new(id.to_string()));

        let query = format!(
            "UPDATE schedules SET {} WHERE id = ?",
            updates.join(", ")
        );

        conn.execute(&query, rusqlite::params_from_iter(params_vec.iter()))?;

        Ok(())
    }

    /// Delete a schedule
    pub fn delete_schedule(&self, id: &str) -> Result<()> {
        let conn = self.get_connection()?;
        conn.execute("DELETE FROM schedules WHERE id = ?1", [id])?;
        Ok(())
    }

    /// Record a new run
    pub fn record_run(
        &self,
        schedule_id: Option<&str>,
        workbook_path: &str,
        project_root: &str,
    ) -> Result<Run> {
        let now = Utc::now().timestamp();
        let id = Uuid::new_v4().to_string();

        let run = Run {
            id: id.clone(),
            schedule_id: schedule_id.map(String::from),
            workbook_path: workbook_path.to_string(),
            project_root: project_root.to_string(),
            started_at: now,
            finished_at: None,
            duration: None,
            status: "running".to_string(),
            error_message: None,
            report_path: None,
        };

        let conn = self.get_connection()?;
        conn.execute(
            "INSERT INTO runs (id, schedule_id, workbook_path, project_root, started_at, finished_at, duration, status, error_message, report_path)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![
                run.id,
                run.schedule_id,
                run.workbook_path,
                run.project_root,
                run.started_at,
                run.finished_at,
                run.duration,
                run.status,
                run.error_message,
                run.report_path,
            ],
        )?;

        Ok(run)
    }

    /// Update a run with completion status
    pub fn complete_run(
        &self,
        run_id: &str,
        status: &str,
        error_message: Option<&str>,
        report_path: Option<&str>,
    ) -> Result<()> {
        let now = Utc::now().timestamp();
        let conn = self.get_connection()?;

        // Get started_at to calculate duration
        let started_at: i64 = conn.query_row(
            "SELECT started_at FROM runs WHERE id = ?1",
            [run_id],
            |row| row.get(0),
        )?;

        let duration = now - started_at;

        conn.execute(
            "UPDATE runs SET finished_at = ?1, duration = ?2, status = ?3, error_message = ?4, report_path = ?5
             WHERE id = ?6",
            params![now, duration, status, error_message, report_path, run_id],
        )?;

        Ok(())
    }

    /// List recent runs
    pub fn list_runs(&self, limit: usize) -> Result<Vec<Run>> {
        let conn = self.get_connection()?;
        let mut stmt = conn.prepare(
            "SELECT id, schedule_id, workbook_path, project_root, started_at, finished_at, duration, status, error_message, report_path
             FROM runs
             ORDER BY started_at DESC
             LIMIT ?1"
        )?;

        let runs = stmt.query_map([limit], |row| {
            Ok(Run {
                id: row.get(0)?,
                schedule_id: row.get(1)?,
                workbook_path: row.get(2)?,
                project_root: row.get(3)?,
                started_at: row.get(4)?,
                finished_at: row.get(5)?,
                duration: row.get(6)?,
                status: row.get(7)?,
                error_message: row.get(8)?,
                report_path: row.get(9)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;

        Ok(runs)
    }

    /// Validate a cron expression
    fn validate_cron(&self, cron_expression: &str) -> Result<()> {
        // Use tokio-cron-scheduler to validate
        let _job = Job::new(cron_expression, |_uuid, _lock| {})
            .context("Invalid cron expression")?;
        Ok(())
    }

    /// Calculate the next run time for a cron expression
    fn calculate_next_run(&self, cron_expression: &str) -> Result<Option<i64>> {
        // Parse cron and get next execution time
        let _job = Job::new(cron_expression, |_uuid, _lock| {})
            .context("Invalid cron expression")?;

        // tokio-cron-scheduler doesn't expose next_tick directly in a simple way
        // For now, return None and let the scheduler handle it
        // TODO: Implement proper next run calculation
        Ok(None)
    }

    /// Update next_run for a schedule after execution
    pub fn update_next_run(&self, schedule_id: &str) -> Result<()> {
        let _schedule = self.get_schedule(schedule_id)?
            .context("Schedule not found")?;

        let now = Utc::now().timestamp();
        let conn = self.get_connection()?;

        conn.execute(
            "UPDATE schedules SET last_run = ?1, modified_at = ?2 WHERE id = ?3",
            params![now, now, schedule_id],
        )?;

        Ok(())
    }

    /// Clean up old runs (keep only the most recent N runs)
    pub fn cleanup_old_runs(&self, keep_count: usize) -> Result<usize> {
        let conn = self.get_connection()?;

        // Delete runs beyond the keep_count
        let deleted = conn.execute(
            "DELETE FROM runs WHERE id NOT IN (
                SELECT id FROM runs ORDER BY started_at DESC LIMIT ?1
            )",
            [keep_count],
        )?;

        Ok(deleted)
    }

    /// Initialize and start the background scheduler
    pub async fn start_scheduler(&mut self) -> Result<()> {
        let sched = JobScheduler::new().await?;
        sched.start().await?;
        self.scheduler = Some(Arc::new(sched));
        Ok(())
    }

    /// Stop the background scheduler
    pub async fn stop_scheduler(&mut self) -> Result<()> {
        if let Some(scheduler) = self.scheduler.take() {
            // Try to get exclusive access to shut down
            if let Ok(mut sched) = Arc::try_unwrap(scheduler) {
                sched.shutdown().await?;
            }
            // If Arc::try_unwrap fails, there are other references
            // The scheduler will be cleaned up when all references are dropped
        }
        Ok(())
    }
}

impl Default for SchedulerManager {
    fn default() -> Self {
        Self::new().expect("Failed to create scheduler manager")
    }
}
