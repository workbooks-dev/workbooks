import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";

export function CreateProject({ onProjectCreated }) {
  const [projectName, setProjectName] = useState("");
  const [projectPath, setProjectPath] = useState("");
  const [creating, setCreating] = useState(false);
  const [error, setError] = useState(null);

  const handleBrowse = async () => {
    try {
      const selected = await open({
        directory: true,
        multiple: false,
        title: "Select Project Folder",
      });

      if (selected) {
        setProjectPath(selected);

        // Auto-populate project name from folder name
        const folderName = selected.split('/').pop() || selected.split('\\').pop();
        if (folderName) {
          setProjectName(folderName);
        }
      }
    } catch (err) {
      console.error("Failed to open folder picker:", err);
      setError("Failed to open folder picker: " + err.toString());
    }
  };

  const handleCreate = async (e) => {
    e.preventDefault();

    if (!projectName || !projectPath) {
      setError("Please provide both project name and folder");
      return;
    }

    setCreating(true);
    setError(null);

    try {
      const project = await invoke("create_project", {
        projectPath,
        projectName,
      });

      console.log("Project created:", project);
      onProjectCreated(project);
    } catch (err) {
      console.error("Failed to create project:", err);
      setError(err.toString());
    } finally {
      setCreating(false);
    }
  };

  return (
    <div className="flex items-center justify-center min-h-[calc(100vh-5rem)] px-6">
      <div className="w-full max-w-xl">
        <div className="bg-white rounded-xl shadow-lg p-10 border border-gray-200">
          <h2 className="text-3xl font-bold text-gray-900 mb-2">Create New Project</h2>
          <p className="text-sm text-gray-600 mb-8">Set up a new Workbooks workspace for your data pipelines</p>

          <form onSubmit={handleCreate} className="space-y-6">
            <div className="space-y-2">
              <label htmlFor="project-path" className="block text-sm font-medium text-gray-700">
                Folder
              </label>
              <div className="flex gap-2">
                <input
                  id="project-path"
                  type="text"
                  value={projectPath}
                  readOnly
                  placeholder="Click Browse to select a folder..."
                  disabled={creating}
                  className="flex-1 px-3 py-2.5 border border-gray-300 rounded-lg bg-gray-50 text-gray-700 focus:outline-none disabled:cursor-not-allowed text-base"
                />
                <button
                  type="button"
                  onClick={handleBrowse}
                  disabled={creating}
                  className="px-4 py-2.5 bg-white border border-gray-300 rounded-lg hover:bg-gray-50 disabled:opacity-50 disabled:cursor-not-allowed font-medium"
                >
                  Browse
                </button>
              </div>
              <p className="text-sm text-gray-500">Select or create a folder for your project</p>
            </div>

            <div className="space-y-2">
              <label htmlFor="project-name" className="block text-sm font-medium text-gray-700">
                Project Name
              </label>
              <input
                id="project-name"
                type="text"
                value={projectName}
                onChange={(e) => setProjectName(e.target.value)}
                placeholder="my-pipeline"
                disabled={creating}
                className="w-full px-3 py-2.5 border border-gray-300 rounded-lg focus:outline-none focus:ring-2 focus:ring-blue-500 focus:border-transparent disabled:bg-gray-100 disabled:cursor-not-allowed text-base"
              />
              <p className="text-sm text-gray-500">Display name (auto-filled from folder name)</p>
            </div>

            {error && (
              <div className="p-3 bg-red-50 border border-red-200 rounded-lg text-red-700 text-sm">
                {error}
              </div>
            )}

            <button
              type="submit"
              disabled={creating || !projectName || !projectPath}
              className="w-full px-4 py-3 bg-blue-600 text-white rounded-lg hover:bg-blue-700 disabled:bg-gray-300 disabled:cursor-not-allowed font-medium text-base shadow-sm transition-all"
            >
              {creating ? "Creating..." : "Create Project"}
            </button>
          </form>
        </div>
      </div>
    </div>
  );
}
