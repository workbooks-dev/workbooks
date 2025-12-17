import { useState, useEffect } from "react";
import { useTether } from "../hooks/useTether";

export function RunLog() {
  const [logs, setLogs] = useState([]);
  const { getRunLogs } = useTether();

  useEffect(() => {
    loadLogs();
  }, []);

  const loadLogs = async () => {
    try {
      const runLogs = await getRunLogs();
      setLogs(runLogs);
    } catch (error) {
      console.error("Failed to load logs:", error);
    }
  };

  return (
    <div className="run-log">
      <h2>Run History</h2>
      <div className="logs">
        {logs.map((log) => (
          <div key={log.id} className={`log-item status-${log.status}`}>
            <div className="log-header">
              <span className="notebook-name">{log.notebook}</span>
              <span className="log-status">{log.status}</span>
            </div>
            <div className="log-time">
              Started: {new Date(log.startedAt).toLocaleString()}
            </div>
            {log.completedAt && (
              <div className="log-time">
                Completed: {new Date(log.completedAt).toLocaleString()}
              </div>
            )}
            {log.error && <div className="log-error">{log.error}</div>}
          </div>
        ))}
      </div>
    </div>
  );
}
