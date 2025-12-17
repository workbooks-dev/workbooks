import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Welcome } from "./components/Welcome";
import { CreateProject } from "./components/CreateProject";
import { FileExplorer } from "./components/FileExplorer";
import { NotebookViewer } from "./components/NotebookViewer";
import "./App.css";

function App() {
  const [currentProject, setCurrentProject] = useState(null);
  const [loading, setLoading] = useState(true);
  const [view, setView] = useState("loading"); // loading, welcome, create, project
  const [openNotebook, setOpenNotebook] = useState(null);

  useEffect(() => {
    checkCurrentProject();
  }, []);

  async function checkCurrentProject() {
    try {
      const project = await invoke("get_current_project");
      if (project) {
        setCurrentProject(project);
        setView("project");
      } else {
        setView("welcome");
      }
    } catch (error) {
      console.error("Failed to get current project:", error);
      setView("welcome");
    } finally {
      setLoading(false);
    }
  }

  function handleProjectOpened(project, mode) {
    if (mode === "create") {
      setView("create");
    } else if (project) {
      setCurrentProject(project);
      setView("project");
    }
  }

  function handleProjectCreated(project) {
    setCurrentProject(project);
    setView("project");
  }

  function handleOpenNotebook(notebookPath) {
    setOpenNotebook(notebookPath);
  }

  function handleCloseNotebook() {
    setOpenNotebook(null);
  }

  if (loading || view === "loading") {
    return (
      <div className="app loading">
        <p>Loading...</p>
      </div>
    );
  }

  if (view === "welcome") {
    return (
      <div className="app welcome-view">
        <Welcome onProjectOpened={handleProjectOpened} />
      </div>
    );
  }

  if (view === "create") {
    return (
      <div className="app">
        <header className="app-header">
          <h1>Tether</h1>
          <button onClick={() => setView("welcome")}>Back</button>
        </header>
        <main className="app-main">
          <CreateProject onProjectCreated={handleProjectCreated} />
        </main>
      </div>
    );
  }

  return (
    <div className="app">
      <div className="app-body">
        <aside className="app-sidebar">
          <FileExplorer
            projectRoot={currentProject.root}
            projectName={currentProject.name}
            onOpenNotebook={handleOpenNotebook}
          />
        </aside>
        <main className="app-main">
          <div className="project-view">
            {openNotebook ? (
              <NotebookViewer
                notebookPath={openNotebook}
                projectRoot={currentProject.root}
                onClose={handleCloseNotebook}
              />
            ) : (
              <div className="placeholder">
                <p>Select a notebook to open</p>
              </div>
            )}
          </div>
        </main>
      </div>
    </div>
  );
}

export default App;
