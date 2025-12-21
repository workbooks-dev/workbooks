import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { ask } from "@tauri-apps/plugin-dialog";

export function ScheduleTab({ projectRoot = null, onClose, initialSubTab = "scheduled", initialShowAllProjects = null }) {
  const [activeSubTab, setActiveSubTab] = useState(initialSubTab); // "scheduled" or "runs"
  const [schedules, setSchedules] = useState([]);
  const [runs, setRuns] = useState([]);
  const [loading, setLoading] = useState(false);
  const [showAddDialog, setShowAddDialog] = useState(false);
  const [editingSchedule, setEditingSchedule] = useState(null);
  // Default to showing all projects if no projectRoot is provided, or if explicitly requested
  const [showAllProjects, setShowAllProjects] = useState(
    initialShowAllProjects !== null ? initialShowAllProjects : !projectRoot
  );

  // Pagination state
  const [currentPage, setCurrentPage] = useState(1);
  const [pageSize, setPageSize] = useState(20);
  const [totalRuns, setTotalRuns] = useState(0);

  // Date filtering state
  const [startDate, setStartDate] = useState("");
  const [endDate, setEndDate] = useState("");

  useEffect(() => {
    loadSchedules();
    loadRuns();

    // Auto-refresh runs every 3 seconds to update running status
    const interval = setInterval(() => {
      loadRuns(true); // Silent refresh (no loading spinner)
    }, 3000);

    return () => clearInterval(interval);
  }, [showAllProjects, projectRoot, currentPage, pageSize, startDate, endDate]); // Reload when toggling view, pagination, or filters change

  const loadSchedules = async () => {
    setLoading(true);
    try {
      const schedulesList = await invoke("list_schedules");
      // Filter schedules based on view mode
      const filteredSchedules = showAllProjects
        ? (schedulesList || [])
        : (schedulesList || []).filter(s => s && s.project_root === projectRoot);
      setSchedules(filteredSchedules);
    } catch (err) {
      console.error("Failed to load schedules:", err);
      setSchedules([]);
    } finally {
      setLoading(false);
    }
  };

  const loadRuns = async (silent = false) => {
    if (!silent) {
      setLoading(true);
    }
    try {
      // Convert date strings to Unix timestamps (seconds)
      const startTime = startDate ? Math.floor(new Date(startDate).getTime() / 1000) : null;
      const endTime = endDate ? Math.floor(new Date(endDate + "T23:59:59").getTime() / 1000) : null;

      // Calculate offset based on current page
      const offset = (currentPage - 1) * pageSize;

      // Get total count first
      const totalCount = await invoke("count_runs", {
        startTime,
        endTime,
      });
      setTotalRuns(totalCount);

      // Get paginated runs
      const runsList = await invoke("list_runs_paginated", {
        limit: pageSize,
        offset,
        startTime,
        endTime,
      });

      // Filter runs based on view mode
      const filteredRuns = showAllProjects
        ? (runsList || [])
        : (runsList || []).filter(r => r && r.project_root === projectRoot);
      setRuns(filteredRuns);
    } catch (err) {
      console.error("Failed to load runs:", err);
      setRuns([]);
      setTotalRuns(0);
    } finally {
      if (!silent) {
        setLoading(false);
      }
    }
  };

  const handleToggleSchedule = async (schedule) => {
    try {
      await invoke("update_schedule", {
        scheduleId: schedule.id,
        cronExpression: null,
        enabled: !schedule.enabled,
      });
      await loadSchedules();
    } catch (err) {
      console.error("Failed to toggle schedule:", err);
      alert(`Failed to toggle schedule: ${err}`);
    }
  };

  const handleDeleteSchedule = async (schedule) => {
    const confirmed = await ask(
      `Are you sure you want to delete the schedule for "${getWorkbookName(schedule.workbook_path)}"?`,
      {
        title: "Delete Schedule",
        kind: "warning",
        okLabel: "Delete",
        cancelLabel: "Cancel",
      }
    );

    if (confirmed) {
      try {
        await invoke("delete_schedule", { scheduleId: schedule.id });
        await loadSchedules();
      } catch (err) {
        console.error("Failed to delete schedule:", err);
        alert(`Failed to delete schedule: ${err}`);
      }
    }
  };

  const handleEditSchedule = (schedule) => {
    setEditingSchedule(schedule);
    setShowAddDialog(true);
  };

  const handleRunNow = async (schedule) => {
    try {
      await invoke("run_schedule_now", { scheduleId: schedule.id });
      // Switch to runs tab
      setActiveSubTab("runs");
      // Reload runs to show the new execution
      setTimeout(() => loadRuns(), 500);
    } catch (err) {
      console.error("Failed to run workbook:", err);
      alert(`Failed to run workbook: ${err}`);
    }
  };

  const getWorkbookName = (path) => {
    if (!path) return "Unknown";
    return path.split("/").pop().replace(".ipynb", "");
  };

  const getProjectName = (projectPath) => {
    if (!projectPath) return "Unknown Project";
    const name = projectPath.split("/").pop();
    return name || projectPath;
  };

  const formatCronExpression = (cron) => {
    const parts = cron.split(" ");

    // Every hour
    if (cron === "0 0 * * * *") {
      return "Every hour";
    }

    // Every X minutes
    if (cron.match(/^0 \*\/\d+ \* \* \* \*$/)) {
      const minutes = parseInt(parts[1].replace("*/", ""));
      return `Every ${minutes} minute${minutes !== 1 ? "s" : ""}`;
    }

    // Every X hours
    if (cron.match(/^0 0 \*\/\d+ \* \* \*$/)) {
      const hours = parseInt(parts[2].replace("*/", ""));
      return `Every ${hours} hour${hours !== 1 ? "s" : ""}`;
    }

    // Daily at specific time
    if (cron.match(/^0 \d+ \d+ \* \* \*$/)) {
      const hour = parseInt(parts[2]);
      const minute = parseInt(parts[1]);
      const period = hour >= 12 ? "PM" : "AM";
      const displayHour = hour === 0 ? 12 : hour > 12 ? hour - 12 : hour;
      return `Daily at ${displayHour}:${minute.toString().padStart(2, "0")} ${period}`;
    }

    // Weekly at specific day and time
    if (cron.match(/^0 \d+ \d+ \* \* \d+$/)) {
      const hour = parseInt(parts[2]);
      const minute = parseInt(parts[1]);
      const day = parseInt(parts[5]);
      const days = ["Sunday", "Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday"];
      const period = hour >= 12 ? "PM" : "AM";
      const displayHour = hour === 0 ? 12 : hour > 12 ? hour - 12 : hour;
      return `${days[day]} at ${displayHour}:${minute.toString().padStart(2, "0")} ${period}`;
    }

    return `Custom: ${cron}`;
  };

  const formatTimestamp = (timestamp) => {
    if (!timestamp) return "Never";
    const date = new Date(timestamp * 1000);
    return date.toLocaleString();
  };

  const formatDuration = (duration) => {
    if (!duration) return "-";
    const seconds = Math.floor(duration / 1000);
    if (seconds < 60) return `${seconds}s`;
    const minutes = Math.floor(seconds / 60);
    const remainingSeconds = seconds % 60;
    return `${minutes}m ${remainingSeconds}s`;
  };

  const getStatusBadge = (status) => {
    const badges = {
      running: (
        <span className="text-xs px-2 py-1 rounded-md font-medium bg-blue-50 text-blue-700 flex items-center gap-1">
          <span className="inline-block w-2 h-2 bg-blue-500 rounded-full animate-pulse"></span>
          Running
        </span>
      ),
      success: (
        <span className="text-xs px-2 py-1 rounded-md font-medium bg-emerald-50 text-emerald-700">
          Success
        </span>
      ),
      failed: (
        <span className="text-xs px-2 py-1 rounded-md font-medium bg-red-50 text-red-700">
          Failed
        </span>
      ),
      interrupted: (
        <span className="text-xs px-2 py-1 rounded-md font-medium bg-amber-50 text-amber-700">
          Interrupted
        </span>
      ),
    };
    return badges[status] || status;
  };

  return (
    <div className="flex flex-col h-full bg-white">
      {/* Header */}
      <div className="border-b border-gray-200 px-6 py-4 flex items-center justify-between">
        <h2 className="text-lg font-semibold text-gray-900">Schedule</h2>
        <div className="flex items-center gap-2">
          <span className="text-sm text-gray-600">View:</span>
          <button
            onClick={() => setShowAllProjects(!showAllProjects)}
            className={`px-3 py-1.5 text-sm font-medium rounded-md transition-all ${
              showAllProjects
                ? "bg-blue-600 text-white"
                : "bg-gray-100 text-gray-700 hover:bg-gray-200"
            }`}
          >
            {showAllProjects ? "All Projects" : "Current Project"}
          </button>
        </div>
      </div>

      {/* Sub-tabs */}
      <div className="border-b border-gray-200 px-6 flex gap-1">
        <button
          onClick={() => setActiveSubTab("scheduled")}
          className={`px-4 py-3 text-sm font-medium transition-all ${
            activeSubTab === "scheduled"
              ? "text-blue-600 border-b-2 border-blue-600"
              : "text-gray-500 hover:text-gray-700"
          }`}
        >
          Scheduled Workbooks
        </button>
        <button
          onClick={() => setActiveSubTab("runs")}
          className={`px-4 py-3 text-sm font-medium transition-all ${
            activeSubTab === "runs"
              ? "text-blue-600 border-b-2 border-blue-600"
              : "text-gray-500 hover:text-gray-700"
          }`}
        >
          Recent Runs
        </button>
      </div>

      {/* Content */}
      <div className="flex-1 overflow-auto">
        {activeSubTab === "scheduled" && (
          <div className="p-6">
            {/* Add Schedule Button */}
            <button
              onClick={() => {
                setEditingSchedule(null);
                setShowAddDialog(true);
              }}
              className="mb-4 px-4 py-2 text-sm font-medium text-white bg-blue-600 hover:bg-blue-700 rounded-md transition-colors shadow-sm"
            >
              + Add Schedule
            </button>

            {/* Schedules Table */}
            {loading ? (
              <div className="text-center py-12 text-gray-500">Loading...</div>
            ) : schedules.length === 0 ? (
              <div className="text-center py-12">
                <div className="text-5xl mb-4">⏰</div>
                <h3 className="text-base font-semibold text-gray-900 mb-2">No scheduled workbooks</h3>
                <p className="text-sm text-gray-600 mb-6">Get started by scheduling your first workbook.</p>
              </div>
            ) : (
              <div className="bg-white border border-gray-200 rounded-lg overflow-hidden">
                <table className="w-full">
                  <thead>
                    <tr className="bg-gray-50 border-b border-gray-200">
                      <th className="text-left px-4 py-3 text-gray-700 text-xs font-semibold uppercase tracking-wider">
                        Workbook
                      </th>
                      {showAllProjects && (
                        <th className="text-left px-4 py-3 text-gray-700 text-xs font-semibold uppercase tracking-wider">
                          Project
                        </th>
                      )}
                      <th className="text-left px-4 py-3 text-gray-700 text-xs font-semibold uppercase tracking-wider">
                        Frequency
                      </th>
                      <th className="text-left px-4 py-3 text-gray-700 text-xs font-semibold uppercase tracking-wider">
                        Next Run
                      </th>
                      <th className="text-left px-4 py-3 text-gray-700 text-xs font-semibold uppercase tracking-wider">
                        Last Run
                      </th>
                      <th className="text-left px-4 py-3 text-gray-700 text-xs font-semibold uppercase tracking-wider">
                        Status
                      </th>
                      <th className="text-left px-4 py-3 text-gray-700 text-xs font-semibold uppercase tracking-wider">
                        Actions
                      </th>
                    </tr>
                  </thead>
                  <tbody>
                    {schedules.map((schedule) => (
                      <tr key={schedule.id} className="border-b border-gray-100 hover:bg-gray-50">
                        <td className="px-4 py-3 text-sm text-gray-900">
                          {getWorkbookName(schedule.workbook_path)}
                        </td>
                        {showAllProjects && (
                          <td className="px-4 py-3 text-sm text-gray-600">
                            {getProjectName(schedule.project_root)}
                          </td>
                        )}
                        <td className="px-4 py-3 text-sm text-gray-700">
                          {formatCronExpression(schedule.cron_expression)}
                        </td>
                        <td className="px-4 py-3 text-sm text-gray-700">
                          {formatTimestamp(schedule.next_run)}
                        </td>
                        <td className="px-4 py-3 text-sm text-gray-700">
                          {formatTimestamp(schedule.last_run)}
                        </td>
                        <td className="px-4 py-3">
                          <label className="flex items-center cursor-pointer">
                            <input
                              type="checkbox"
                              checked={schedule.enabled}
                              onChange={() => handleToggleSchedule(schedule)}
                              className="w-4 h-4 text-blue-600 border-gray-300 rounded focus:ring-blue-500"
                            />
                            <span className="ml-2 text-sm text-gray-700">
                              {schedule.enabled ? "Enabled" : "Disabled"}
                            </span>
                          </label>
                        </td>
                        <td className="px-4 py-3">
                          <div className="flex gap-2">
                            <button
                              onClick={() => handleRunNow(schedule)}
                              className="px-3 py-1 text-xs font-medium text-white bg-blue-600 hover:bg-blue-700 rounded-md transition-all shadow-sm"
                              title="Run this workbook now"
                            >
                              Run Now
                            </button>
                            <button
                              onClick={() => handleEditSchedule(schedule)}
                              className="px-3 py-1 text-xs font-medium text-gray-700 bg-white hover:bg-gray-50 border border-gray-300 rounded-md transition-all"
                            >
                              Edit
                            </button>
                            <button
                              onClick={() => handleDeleteSchedule(schedule)}
                              className="px-3 py-1 text-xs font-medium text-red-700 bg-white hover:bg-red-50 border border-red-300 rounded-md transition-all"
                            >
                              Delete
                            </button>
                          </div>
                        </td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>
            )}
          </div>
        )}

        {activeSubTab === "runs" && (
          <div className="p-6">
            {/* Filters and Controls */}
            <div className="mb-4 flex flex-wrap items-end gap-4">
              {/* Date Range Filter */}
              <div className="flex gap-2 items-end">
                <div>
                  <label className="block text-xs font-medium text-gray-700 mb-1">Start Date</label>
                  <input
                    type="date"
                    value={startDate}
                    onChange={(e) => {
                      setStartDate(e.target.value);
                      setCurrentPage(1); // Reset to first page when filtering
                    }}
                    className="px-3 py-1.5 text-sm border border-gray-300 rounded-md focus:outline-none focus:ring-2 focus:ring-blue-500"
                  />
                </div>
                <div>
                  <label className="block text-xs font-medium text-gray-700 mb-1">End Date</label>
                  <input
                    type="date"
                    value={endDate}
                    onChange={(e) => {
                      setEndDate(e.target.value);
                      setCurrentPage(1); // Reset to first page when filtering
                    }}
                    className="px-3 py-1.5 text-sm border border-gray-300 rounded-md focus:outline-none focus:ring-2 focus:ring-blue-500"
                  />
                </div>
                {(startDate || endDate) && (
                  <button
                    onClick={() => {
                      setStartDate("");
                      setEndDate("");
                      setCurrentPage(1);
                    }}
                    className="px-3 py-1.5 text-sm text-gray-700 bg-white hover:bg-gray-50 border border-gray-300 rounded-md transition-all"
                  >
                    Clear
                  </button>
                )}
              </div>

              {/* Page Size Selector */}
              <div className="ml-auto">
                <label className="block text-xs font-medium text-gray-700 mb-1">Page Size</label>
                <select
                  value={pageSize}
                  onChange={(e) => {
                    setPageSize(Number(e.target.value));
                    setCurrentPage(1); // Reset to first page when changing page size
                  }}
                  className="px-3 py-1.5 text-sm border border-gray-300 rounded-md focus:outline-none focus:ring-2 focus:ring-blue-500"
                >
                  <option value="10">10</option>
                  <option value="20">20</option>
                  <option value="50">50</option>
                  <option value="100">100</option>
                </select>
              </div>
            </div>

            {/* Runs Table */}
            {loading ? (
              <div className="text-center py-12 text-gray-500">Loading...</div>
            ) : runs.length === 0 ? (
              <div className="text-center py-12">
                <div className="text-5xl mb-4">📊</div>
                <h3 className="text-base font-semibold text-gray-900 mb-2">No run history</h3>
                <p className="text-sm text-gray-600 mb-6">
                  {startDate || endDate
                    ? "No runs found for the selected date range."
                    : "Run history will appear here once you schedule and execute workbooks."}
                </p>
              </div>
            ) : (
              <>
                <div className="bg-white border border-gray-200 rounded-lg overflow-hidden">
                <table className="w-full">
                  <thead>
                    <tr className="bg-gray-50 border-b border-gray-200">
                      <th className="w-8"></th>
                      <th className="text-left px-4 py-3 text-gray-700 text-xs font-semibold uppercase tracking-wider">
                        Workbook
                      </th>
                      {showAllProjects && (
                        <th className="text-left px-4 py-3 text-gray-700 text-xs font-semibold uppercase tracking-wider">
                          Project
                        </th>
                      )}
                      <th className="text-left px-4 py-3 text-gray-700 text-xs font-semibold uppercase tracking-wider">
                        Started At
                      </th>
                      <th className="text-left px-4 py-3 text-gray-700 text-xs font-semibold uppercase tracking-wider">
                        Duration
                      </th>
                      <th className="text-left px-4 py-3 text-gray-700 text-xs font-semibold uppercase tracking-wider">
                        Status
                      </th>
                      <th className="text-left px-4 py-3 text-gray-700 text-xs font-semibold uppercase tracking-wider">
                        Cells
                      </th>
                    </tr>
                  </thead>
                  <tbody>
                    {runs.map((run) => (
                      <RunRow
                        key={run.id}
                        run={run}
                        showAllProjects={showAllProjects}
                        getWorkbookName={getWorkbookName}
                        getProjectName={getProjectName}
                        formatTimestamp={formatTimestamp}
                        formatDuration={formatDuration}
                        getStatusBadge={getStatusBadge}
                      />
                    ))}
                  </tbody>
                </table>
              </div>

              {/* Pagination Controls */}
              <div className="mt-4 flex items-center justify-between">
                <div className="text-sm text-gray-700">
                  Showing {runs.length > 0 ? ((currentPage - 1) * pageSize + 1) : 0} to{" "}
                  {Math.min(currentPage * pageSize, totalRuns)} of {totalRuns} runs
                </div>
                <div className="flex gap-2">
                  <button
                    onClick={() => setCurrentPage(currentPage - 1)}
                    disabled={currentPage === 1}
                    className="px-3 py-1.5 text-sm font-medium text-gray-700 bg-white hover:bg-gray-50 border border-gray-300 rounded-md transition-all disabled:opacity-50 disabled:cursor-not-allowed"
                  >
                    Previous
                  </button>
                  <div className="flex items-center gap-1">
                    {Array.from({ length: Math.ceil(totalRuns / pageSize) }, (_, i) => i + 1)
                      .filter(page => {
                        // Show first, last, current, and adjacent pages
                        const totalPages = Math.ceil(totalRuns / pageSize);
                        return (
                          page === 1 ||
                          page === totalPages ||
                          Math.abs(page - currentPage) <= 1
                        );
                      })
                      .map((page, index, array) => (
                        <div key={page} className="flex items-center">
                          {index > 0 && array[index - 1] !== page - 1 && (
                            <span className="px-2 text-gray-400">...</span>
                          )}
                          <button
                            onClick={() => setCurrentPage(page)}
                            className={`px-3 py-1.5 text-sm font-medium rounded-md transition-all ${
                              currentPage === page
                                ? "bg-blue-600 text-white"
                                : "text-gray-700 bg-white hover:bg-gray-50 border border-gray-300"
                            }`}
                          >
                            {page}
                          </button>
                        </div>
                      ))}
                  </div>
                  <button
                    onClick={() => setCurrentPage(currentPage + 1)}
                    disabled={currentPage >= Math.ceil(totalRuns / pageSize)}
                    className="px-3 py-1.5 text-sm font-medium text-gray-700 bg-white hover:bg-gray-50 border border-gray-300 rounded-md transition-all disabled:opacity-50 disabled:cursor-not-allowed"
                  >
                    Next
                  </button>
                </div>
              </div>
            </>
            )}
          </div>
        )}
      </div>

      {/* Add/Edit Schedule Dialog */}
      {showAddDialog && (
        <AddEditScheduleDialog
          projectRoot={projectRoot}
          schedule={editingSchedule}
          onClose={() => {
            setShowAddDialog(false);
            setEditingSchedule(null);
          }}
          onSuccess={() => {
            setShowAddDialog(false);
            setEditingSchedule(null);
            loadSchedules();
          }}
        />
      )}
    </div>
  );
}

// Run Row Component with Expandable Details
function RunRow({ run, showAllProjects, getWorkbookName, getProjectName, formatTimestamp, formatDuration, getStatusBadge }) {
  const [expanded, setExpanded] = useState(false);

  // Parse metadata JSON
  const metadata = run.metadata ? JSON.parse(run.metadata) : null;

  return (
    <>
      <tr
        className="border-b border-gray-100 hover:bg-gray-50 cursor-pointer"
        onClick={() => metadata && setExpanded(!expanded)}
      >
        <td className="px-2 py-3 text-center">
          {metadata && (
            <span className="text-gray-400 text-xs">
              {expanded ? "▼" : "▶"}
            </span>
          )}
        </td>
        <td className="px-4 py-3 text-sm text-gray-900">
          {getWorkbookName(run.workbook_path)}
        </td>
        {showAllProjects && (
          <td className="px-4 py-3 text-sm text-gray-600">
            {getProjectName(run.project_root)}
          </td>
        )}
        <td className="px-4 py-3 text-sm text-gray-700">
          {formatTimestamp(run.started_at)}
        </td>
        <td className="px-4 py-3 text-sm text-gray-700">
          {formatDuration(run.duration)}
        </td>
        <td className="px-4 py-3">{getStatusBadge(run.status)}</td>
        <td className="px-4 py-3">
          {metadata ? (
            <span className="text-xs text-gray-600">
              {metadata.cells_succeeded}/{metadata.cells_executed}
            </span>
          ) : (
            <span className="text-xs text-gray-400">—</span>
          )}
        </td>
      </tr>

      {/* Expanded Details */}
      {expanded && metadata && (
        <tr className="bg-gray-50">
          <td colSpan={showAllProjects ? 7 : 6} className="px-12 py-4">
            <div className="space-y-3">
              {/* Execution Summary */}
              <div>
                <h4 className="text-xs font-semibold text-gray-700 uppercase tracking-wider mb-2">
                  Execution Summary
                </h4>
                <div className="grid grid-cols-3 gap-4">
                  <div className="bg-white rounded-md px-3 py-2 border border-gray-200">
                    <div className="text-xs text-gray-500">Cells Executed</div>
                    <div className="text-lg font-semibold text-gray-900">{metadata.cells_executed}</div>
                  </div>
                  <div className="bg-white rounded-md px-3 py-2 border border-gray-200">
                    <div className="text-xs text-gray-500">Succeeded</div>
                    <div className="text-lg font-semibold text-emerald-600">{metadata.cells_succeeded}</div>
                  </div>
                  <div className="bg-white rounded-md px-3 py-2 border border-gray-200">
                    <div className="text-xs text-gray-500">Failed</div>
                    <div className="text-lg font-semibold text-red-600">{metadata.cells_failed}</div>
                  </div>
                </div>
              </div>

              {/* Final Outputs */}
              {metadata.final_outputs && metadata.final_outputs.length > 0 && (
                <div>
                  <h4 className="text-xs font-semibold text-gray-700 uppercase tracking-wider mb-2">
                    Final Cell Outputs
                  </h4>
                  <div className="bg-white rounded-md border border-gray-200 overflow-hidden">
                    <div className="max-h-48 overflow-y-auto">
                      {metadata.final_outputs.map((output, idx) => (
                        <div
                          key={idx}
                          className={`px-3 py-2 text-xs font-mono text-gray-700 ${
                            idx < metadata.final_outputs.length - 1 ? "border-b border-gray-100" : ""
                          }`}
                        >
                          {output}
                        </div>
                      ))}
                    </div>
                  </div>
                </div>
              )}

              {/* Error Message */}
              {run.error_message && (
                <div>
                  <h4 className="text-xs font-semibold text-gray-700 uppercase tracking-wider mb-2">
                    Error Message
                  </h4>
                  <div className="bg-red-50 rounded-md px-3 py-2 border border-red-200">
                    <pre className="text-xs font-mono text-red-700 whitespace-pre-wrap">
                      {run.error_message}
                    </pre>
                  </div>
                </div>
              )}
            </div>
          </td>
        </tr>
      )}
    </>
  );
}

// Add/Edit Schedule Dialog Component
function AddEditScheduleDialog({ projectRoot, schedule, onClose, onSuccess }) {
  const [workbookPath, setWorkbookPath] = useState(schedule?.workbook_path || "");
  const [frequency, setFrequency] = useState("daily"); // daily, hourly, weekly, interval, custom
  const [customCron, setCustomCron] = useState(schedule?.cron_expression || "");
  const [enabled, setEnabled] = useState(schedule?.enabled ?? true);
  const [workbooks, setWorkbooks] = useState([]);
  const [loading, setLoading] = useState(false);

  // Time picker states
  const [dailyHour, setDailyHour] = useState(9);
  const [dailyMinute, setDailyMinute] = useState(0);
  const [weeklyDay, setWeeklyDay] = useState(1); // 1 = Monday
  const [weeklyHour, setWeeklyHour] = useState(9);
  const [weeklyMinute, setWeeklyMinute] = useState(0);

  // Interval states
  const [intervalValue, setIntervalValue] = useState(5);
  const [intervalUnit, setIntervalUnit] = useState("minutes"); // minutes, hours

  useEffect(() => {
    loadWorkbooks();

    // If editing, detect frequency from cron expression and parse values
    if (schedule) {
      const cron = schedule.cron_expression;
      const parts = cron.split(" ");

      // Try to detect preset patterns
      if (cron.match(/^0 0 \d+ \* \* \*$/)) {
        // Daily at specific hour
        setFrequency("daily");
        setDailyHour(parseInt(parts[2]));
        setDailyMinute(0);
      } else if (cron.match(/^0 \d+ \d+ \* \* \*$/)) {
        // Daily at specific hour and minute
        setFrequency("daily");
        setDailyHour(parseInt(parts[2]));
        setDailyMinute(parseInt(parts[1]));
      } else if (cron === "0 0 * * * *") {
        // Hourly
        setFrequency("hourly");
      } else if (cron.match(/^0 0 \d+ \* \* \d+$/)) {
        // Weekly at specific day and hour
        setFrequency("weekly");
        setWeeklyDay(parseInt(parts[5]));
        setWeeklyHour(parseInt(parts[2]));
        setWeeklyMinute(0);
      } else if (cron.match(/^0 \d+ \d+ \* \* \d+$/)) {
        // Weekly at specific day, hour, and minute
        setFrequency("weekly");
        setWeeklyDay(parseInt(parts[5]));
        setWeeklyHour(parseInt(parts[2]));
        setWeeklyMinute(parseInt(parts[1]));
      } else if (cron.match(/^0 \*\/\d+ \* \* \* \*$/)) {
        // Every X minutes
        setFrequency("interval");
        setIntervalValue(parseInt(parts[1].replace("*/", "")));
        setIntervalUnit("minutes");
      } else if (cron.match(/^0 0 \*\/\d+ \* \* \*$/)) {
        // Every X hours
        setFrequency("interval");
        setIntervalValue(parseInt(parts[2].replace("*/", "")));
        setIntervalUnit("hours");
      } else {
        setFrequency("custom");
        setCustomCron(cron);
      }
    }
  }, [schedule]);

  const loadWorkbooks = async () => {
    try {
      const notebooksDir = `${projectRoot}/notebooks`;
      const fileList = await invoke("list_files", {
        directoryPath: notebooksDir,
      });
      const ipynbFiles = fileList.filter((f) => f.extension === "ipynb");
      setWorkbooks(ipynbFiles);
    } catch (err) {
      console.error("Failed to load workbooks:", err);
    }
  };

  const getCronExpression = () => {
    if (frequency === "custom") return customCron;

    if (frequency === "daily") {
      // 0 minute hour * * * (daily at specific time)
      return `0 ${dailyMinute} ${dailyHour} * * *`;
    }

    if (frequency === "hourly") {
      // 0 0 * * * * (every hour)
      return "0 0 * * * *";
    }

    if (frequency === "weekly") {
      // 0 minute hour * * day (weekly on specific day and time)
      return `0 ${weeklyMinute} ${weeklyHour} * * ${weeklyDay}`;
    }

    if (frequency === "interval") {
      if (intervalUnit === "minutes") {
        // 0 */X * * * * (every X minutes)
        return `0 */${intervalValue} * * * *`;
      } else {
        // 0 0 */X * * * (every X hours)
        return `0 0 */${intervalValue} * * *`;
      }
    }

    return "0 0 9 * * *"; // Default: daily at 9am
  };

  const handleSubmit = async (e) => {
    e.preventDefault();
    setLoading(true);

    try {
      const cronExpression = getCronExpression();

      if (schedule) {
        // Update existing schedule
        await invoke("update_schedule", {
          scheduleId: schedule.id,
          cronExpression: cronExpression,
          enabled: enabled,
        });
      } else {
        // Create new schedule (always enabled by default)
        await invoke("add_schedule", {
          projectRoot,
          workbookPath,
          cronExpression,
        });
      }

      onSuccess();
    } catch (err) {
      console.error("Failed to save schedule:", err);
      alert(`Failed to save schedule: ${err}`);
    } finally {
      setLoading(false);
    }
  };

  return (
    <div
      className="fixed inset-0 bg-black bg-opacity-50 flex items-center justify-center z-50"
      onClick={onClose}
    >
      <div
        className="bg-white rounded-lg shadow-xl max-w-md w-full p-6"
        onClick={(e) => e.stopPropagation()}
      >
        <h3 className="text-lg font-semibold text-gray-900 mb-4">
          {schedule ? "Edit Schedule" : "Add Schedule"}
        </h3>

        <form onSubmit={handleSubmit} className="flex flex-col gap-4">
          {/* Workbook Selector (only for new schedules) */}
          {!schedule && (
            <div>
              <label className="block text-sm font-medium text-gray-700 mb-1">Workbook</label>
              <select
                value={workbookPath}
                onChange={(e) => setWorkbookPath(e.target.value)}
                className="w-full px-3 py-2 text-sm border border-gray-300 rounded-md focus:outline-none focus:ring-2 focus:ring-blue-500"
                required
              >
                <option value="">Select a workbook...</option>
                {workbooks.map((wb) => (
                  <option key={wb.path} value={wb.path}>
                    {wb.name.replace(".ipynb", "")}
                  </option>
                ))}
              </select>
            </div>
          )}

          {/* Frequency Selector */}
          <div>
            <label className="block text-sm font-medium text-gray-700 mb-1">Schedule Type</label>
            <select
              value={frequency}
              onChange={(e) => setFrequency(e.target.value)}
              className="w-full px-3 py-2 text-sm border border-gray-300 rounded-md focus:outline-none focus:ring-2 focus:ring-blue-500"
            >
              <option value="interval">Every few minutes/hours</option>
              <option value="daily">Daily at specific time</option>
              <option value="hourly">Every hour</option>
              <option value="weekly">Weekly on specific day</option>
              <option value="custom">Custom (advanced)</option>
            </select>
          </div>

          {/* Interval Inputs */}
          {frequency === "interval" && (
            <div>
              <label className="block text-sm font-medium text-gray-700 mb-1">Run every:</label>
              <div className="flex gap-2">
                <input
                  type="number"
                  min="1"
                  max={intervalUnit === "minutes" ? "59" : "23"}
                  value={intervalValue}
                  onChange={(e) => setIntervalValue(parseInt(e.target.value) || 1)}
                  className="w-24 px-3 py-2 text-sm border border-gray-300 rounded-md focus:outline-none focus:ring-2 focus:ring-blue-500"
                />
                <select
                  value={intervalUnit}
                  onChange={(e) => {
                    setIntervalUnit(e.target.value);
                    // Reset value to safe range when switching units
                    if (e.target.value === "minutes" && intervalValue > 59) {
                      setIntervalValue(5);
                    } else if (e.target.value === "hours" && intervalValue > 23) {
                      setIntervalValue(1);
                    }
                  }}
                  className="flex-1 px-3 py-2 text-sm border border-gray-300 rounded-md focus:outline-none focus:ring-2 focus:ring-blue-500"
                >
                  <option value="minutes">minutes</option>
                  <option value="hours">hours</option>
                </select>
              </div>
              <p className="text-xs text-gray-500 mt-1">
                Workbook will run every {intervalValue} {intervalUnit}
              </p>
            </div>
          )}

          {/* Daily Time Picker */}
          {frequency === "daily" && (
            <div>
              <label className="block text-sm font-medium text-gray-700 mb-1">Time</label>
              <div className="flex gap-2 items-center">
                <select
                  value={dailyHour}
                  onChange={(e) => setDailyHour(parseInt(e.target.value))}
                  className="px-3 py-2 text-sm border border-gray-300 rounded-md focus:outline-none focus:ring-2 focus:ring-blue-500"
                >
                  {Array.from({ length: 24 }, (_, i) => (
                    <option key={i} value={i}>
                      {i.toString().padStart(2, "0")}
                    </option>
                  ))}
                </select>
                <span className="text-gray-500">:</span>
                <select
                  value={dailyMinute}
                  onChange={(e) => setDailyMinute(parseInt(e.target.value))}
                  className="px-3 py-2 text-sm border border-gray-300 rounded-md focus:outline-none focus:ring-2 focus:ring-blue-500"
                >
                  {Array.from({ length: 60 }, (_, i) => (
                    <option key={i} value={i}>
                      {i.toString().padStart(2, "0")}
                    </option>
                  ))}
                </select>
              </div>
              <p className="text-xs text-gray-500 mt-1">
                Workbook will run daily at {dailyHour.toString().padStart(2, "0")}:
                {dailyMinute.toString().padStart(2, "0")}
              </p>
            </div>
          )}

          {/* Weekly Day and Time Picker */}
          {frequency === "weekly" && (
            <div className="space-y-3">
              <div>
                <label className="block text-sm font-medium text-gray-700 mb-1">Day of Week</label>
                <select
                  value={weeklyDay}
                  onChange={(e) => setWeeklyDay(parseInt(e.target.value))}
                  className="w-full px-3 py-2 text-sm border border-gray-300 rounded-md focus:outline-none focus:ring-2 focus:ring-blue-500"
                >
                  <option value="0">Sunday</option>
                  <option value="1">Monday</option>
                  <option value="2">Tuesday</option>
                  <option value="3">Wednesday</option>
                  <option value="4">Thursday</option>
                  <option value="5">Friday</option>
                  <option value="6">Saturday</option>
                </select>
              </div>
              <div>
                <label className="block text-sm font-medium text-gray-700 mb-1">Time</label>
                <div className="flex gap-2 items-center">
                  <select
                    value={weeklyHour}
                    onChange={(e) => setWeeklyHour(parseInt(e.target.value))}
                    className="px-3 py-2 text-sm border border-gray-300 rounded-md focus:outline-none focus:ring-2 focus:ring-blue-500"
                  >
                    {Array.from({ length: 24 }, (_, i) => (
                      <option key={i} value={i}>
                        {i.toString().padStart(2, "0")}
                      </option>
                    ))}
                  </select>
                  <span className="text-gray-500">:</span>
                  <select
                    value={weeklyMinute}
                    onChange={(e) => setWeeklyMinute(parseInt(e.target.value))}
                    className="px-3 py-2 text-sm border border-gray-300 rounded-md focus:outline-none focus:ring-2 focus:ring-blue-500"
                  >
                    {Array.from({ length: 60 }, (_, i) => (
                      <option key={i} value={i}>
                        {i.toString().padStart(2, "0")}
                      </option>
                    ))}
                  </select>
                </div>
              </div>
              <p className="text-xs text-gray-500">
                Workbook will run every{" "}
                {["Sunday", "Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday"][weeklyDay]}{" "}
                at {weeklyHour.toString().padStart(2, "0")}:{weeklyMinute.toString().padStart(2, "0")}
              </p>
            </div>
          )}

          {/* Custom Cron Input */}
          {frequency === "custom" && (
            <div>
              <label className="block text-sm font-medium text-gray-700 mb-1">
                Cron Expression
              </label>
              <input
                type="text"
                value={customCron}
                onChange={(e) => setCustomCron(e.target.value)}
                placeholder="0 0 9 * * *"
                className="w-full px-3 py-2 text-sm border border-gray-300 rounded-md focus:outline-none focus:ring-2 focus:ring-blue-500"
                required
              />
              <p className="text-xs text-gray-500 mt-1">
                Format: second minute hour day month weekday
              </p>
              <p className="text-xs text-gray-400 mt-1">
                Example: "0 15 9 * * 1" = Monday at 9:15 AM
              </p>
            </div>
          )}

          {/* Enabled Checkbox (only show when editing) */}
          {schedule && (
            <div>
              <label className="flex items-center cursor-pointer">
                <input
                  type="checkbox"
                  checked={enabled}
                  onChange={(e) => setEnabled(e.target.checked)}
                  className="w-4 h-4 text-blue-600 border-gray-300 rounded focus:ring-blue-500"
                />
                <span className="ml-2 text-sm text-gray-700">Enabled</span>
              </label>
            </div>
          )}

          {/* Sleep mode notice */}
          <div className="bg-amber-50 border border-amber-200 rounded-md px-3 py-2">
            <p className="text-xs text-amber-800">
              <span className="font-medium">Note:</span> Scheduled tasks will not run when your computer is in system sleep (lid closed or Sleep mode). Display sleep is fine.
            </p>
          </div>

          {/* Actions */}
          <div className="flex gap-2 mt-4">
            <button
              type="submit"
              disabled={loading}
              className="flex-1 px-4 py-2 text-sm font-medium text-white bg-blue-600 hover:bg-blue-700 rounded-md transition-colors disabled:opacity-50 disabled:cursor-not-allowed"
            >
              {loading ? "Saving..." : schedule ? "Update" : "Create"}
            </button>
            <button
              type="button"
              onClick={onClose}
              className="flex-1 px-4 py-2 text-sm font-medium text-gray-700 bg-white hover:bg-gray-50 border border-gray-300 rounded-md transition-all"
            >
              Cancel
            </button>
          </div>
        </form>
      </div>
    </div>
  );
}
