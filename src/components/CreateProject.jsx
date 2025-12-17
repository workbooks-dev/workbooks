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
        title: "Select Project Location",
      });

      if (selected) {
        setProjectPath(selected);

        // Auto-generate project name from selected folder if not set
        if (!projectName && selected) {
          const folderName = selected.split('/').pop() || selected.split('\\').pop();
          if (folderName) {
            setProjectName(folderName);
          }
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
      setError("Please provide both project name and location");
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
    <div className="create-project">
      <h2>Create New Tether Project</h2>
      <form onSubmit={handleCreate}>
        <div className="form-group">
          <label htmlFor="project-name">Project Name</label>
          <input
            id="project-name"
            type="text"
            value={projectName}
            onChange={(e) => setProjectName(e.target.value)}
            placeholder="my-pipeline"
            disabled={creating}
          />
        </div>

        <div className="form-group">
          <label htmlFor="project-path">Project Location</label>
          <div className="path-picker">
            <input
              id="project-path"
              type="text"
              value={projectPath}
              readOnly
              placeholder="Click Browse to select location..."
              disabled={creating}
            />
            <button
              type="button"
              onClick={handleBrowse}
              disabled={creating}
            >
              Browse
            </button>
          </div>
          <small>Select a folder where the project will be created</small>
        </div>

        {error && (
          <div className="error-message">
            {error}
          </div>
        )}

        <button type="submit" disabled={creating || !projectName || !projectPath}>
          {creating ? "Creating..." : "Create Project"}
        </button>
      </form>
    </div>
  );
}
