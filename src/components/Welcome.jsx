import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";

export function Welcome({ onProjectOpened }) {
  const [error, setError] = useState(null);
  const [loading, setLoading] = useState(false);
  const [statusMessage, setStatusMessage] = useState("");

  const handleOpenFolder = async () => {
    setLoading(true);
    setError(null);
    setStatusMessage("");

    try {
      setStatusMessage("Opening folder...");
      const selected = await open({
        directory: true,
        multiple: false,
        title: "Open Folder",
      });

      if (selected) {
        const project = await invoke("open_folder", {
          folderPath: selected,
        });

        console.log("Folder opened:", project);

        // Initialize Python environment and sync dependencies
        setStatusMessage("Syncing dependencies with uv...");
        console.log("Initializing Python environment...");
        await invoke("init_python_env", {
          projectPath: project.root,
        });
        console.log("Python environment initialized");

        setStatusMessage("");
        onProjectOpened(project);
      } else {
        setLoading(false);
        setStatusMessage("");
      }
    } catch (err) {
      console.error("Failed to open folder:", err);
      setError(err.toString());
      setStatusMessage("");
    } finally {
      setLoading(false);
    }
  };

  const handleCreateNew = () => {
    onProjectOpened(null, "create");
  };

  return (
    <div className="welcome">
      <div className="welcome-content">
        <h1>Tether</h1>
        <p className="tagline">Durable notebook orchestration for local-first data pipelines</p>

        <div className="welcome-actions">
          <button
            className="welcome-button primary"
            onClick={handleOpenFolder}
            disabled={loading}
          >
            <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
              <path d="M22 19a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h5l2 3h9a2 2 0 0 1 2 2z"></path>
            </svg>
            Open Folder
          </button>

          <button
            className="welcome-button"
            onClick={handleCreateNew}
            disabled={loading}
          >
            <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
              <path d="M12 5v14M5 12h14"></path>
            </svg>
            Create New Project
          </button>
        </div>

        {statusMessage && (
          <div style={{ marginTop: "1rem", textAlign: "center", color: "#007aff" }}>
            {statusMessage}
          </div>
        )}

        {error && (
          <div className="error-message">
            {error}
          </div>
        )}

        <div className="welcome-info">
          <h3>Getting Started</h3>
          <ul>
            <li><strong>Open Folder:</strong> Open any folder with a pyproject.toml (uv project)</li>
            <li><strong>Create New:</strong> Initialize a new Tether project from scratch</li>
          </ul>
        </div>
      </div>
    </div>
  );
}
