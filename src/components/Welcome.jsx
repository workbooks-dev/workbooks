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
    <div className="flex items-center justify-center min-h-screen bg-gradient-to-br from-gray-50 to-gray-100">
      <div className="w-full max-w-xl px-6">
        <div className="bg-white rounded-xl shadow-soft-lg p-10 border border-gray-200">
          <h1 className="text-4xl font-bold text-gray-900 mb-2">Tether</h1>
          <p className="text-base text-gray-600 mb-8">Durable notebook orchestration for local-first data pipelines</p>

          <div className="space-y-3 mb-6">
            <button
              className="w-full flex items-center justify-center gap-3 px-6 py-3.5 text-base font-medium text-white bg-blue-600 hover:bg-blue-700 rounded-lg transition-all shadow-sm disabled:opacity-50 disabled:cursor-not-allowed"
              onClick={handleOpenFolder}
              disabled={loading}
            >
              <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                <path d="M22 19a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h5l2 3h9a2 2 0 0 1 2 2z"></path>
              </svg>
              Open Folder
            </button>

            <button
              className="w-full flex items-center justify-center gap-3 px-6 py-3.5 text-base font-medium text-gray-700 bg-white hover:bg-gray-50 border border-gray-300 rounded-lg transition-all shadow-sm disabled:opacity-50 disabled:cursor-not-allowed"
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
            <div className="mt-4 text-center text-blue-600 text-sm font-medium">
              {statusMessage}
            </div>
          )}

          {error && (
            <div className="mt-4 px-4 py-3 bg-red-50 border border-red-200 rounded-lg text-red-800 text-sm">
              {error}
            </div>
          )}

          <div className="mt-8 pt-8 border-t border-gray-200">
            <h3 className="text-xs font-semibold uppercase tracking-wider text-gray-500 mb-4">Getting Started</h3>
            <ul className="space-y-2 text-sm text-gray-600">
              <li><span className="font-semibold text-gray-900">Open Folder:</span> Open any folder with a pyproject.toml (uv project)</li>
              <li><span className="font-semibold text-gray-900">Create New:</span> Initialize a new Tether project from scratch</li>
            </ul>
          </div>
        </div>
      </div>
    </div>
  );
}
