import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Welcome } from "./components/Welcome";
import { CreateProject } from "./components/CreateProject";
import { FileExplorer } from "./components/FileExplorer";
import { WorkbookViewer } from "./components/WorkbookViewer";
import { FileViewer } from "./components/FileViewer";
import { TabBar } from "./components/TabBar";
import "./App.css";

function App() {
  const [currentProject, setCurrentProject] = useState(null);
  const [loading, setLoading] = useState(true);
  const [view, setView] = useState("loading"); // loading, welcome, create, project
  const [tabs, setTabs] = useState([]);
  const [activeTabId, setActiveTabId] = useState(null);
  const [autosaveEnabled, setAutosaveEnabled] = useState(true);

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

  function handleOpenFile(filePath, fileType) {
    // Check if file is already open
    const existingTab = tabs.find((tab) => tab.path === filePath);
    if (existingTab) {
      setActiveTabId(existingTab.id);
      return;
    }

    // Create new tab
    const newTab = {
      id: Date.now().toString(),
      path: filePath,
      type: fileType,
      hasUnsavedChanges: false,
    };

    setTabs([...tabs, newTab]);
    setActiveTabId(newTab.id);
  }

  function handleTabSelect(tabId) {
    setActiveTabId(tabId);
  }

  function handleTabClose(tabId) {
    const newTabs = tabs.filter((tab) => tab.id !== tabId);
    setTabs(newTabs);

    // If closing active tab, switch to another tab
    if (tabId === activeTabId) {
      if (newTabs.length > 0) {
        setActiveTabId(newTabs[newTabs.length - 1].id);
      } else {
        setActiveTabId(null);
      }
    }
  }

  function updateTabUnsavedState(tabId, hasUnsavedChanges) {
    setTabs((prevTabs) =>
      prevTabs.map((tab) =>
        tab.id === tabId ? { ...tab, hasUnsavedChanges } : tab
      )
    );
  }

  function handleFileDeleted(filePath) {
    // Close any tabs for this deleted file
    const tab = tabs.find((t) => t.path === filePath);
    if (tab) {
      handleTabClose(tab.id);
    }
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

  const activeTab = tabs.find((tab) => tab.id === activeTabId);

  return (
    <div className="app">
      <div className="app-body">
        <aside className="app-sidebar">
          <FileExplorer
            projectRoot={currentProject.root}
            projectName={currentProject.name}
            onOpenWorkbook={handleOpenFile}
            onFileDeleted={handleFileDeleted}
          />
        </aside>
        <main className="app-main">
          <TabBar
            tabs={tabs}
            activeTabId={activeTabId}
            onTabSelect={handleTabSelect}
            onTabClose={handleTabClose}
            autosaveEnabled={autosaveEnabled}
            onAutosaveToggle={setAutosaveEnabled}
          />
          <div className="project-view">
            {activeTab ? (
              activeTab.type === "workbook" ? (
                <WorkbookViewer
                  key={activeTab.id}
                  workbookPath={activeTab.path}
                  projectRoot={currentProject.root}
                  autosaveEnabled={autosaveEnabled}
                  onClose={() => handleTabClose(activeTab.id)}
                />
              ) : (
                <FileViewer
                  key={activeTab.id}
                  filePath={activeTab.path}
                  onClose={() => handleTabClose(activeTab.id)}
                />
              )
            ) : (
              <div className="placeholder">
                <p>Select a file to open</p>
              </div>
            )}
          </div>
        </main>
      </div>
    </div>
  );
}

export default App;
