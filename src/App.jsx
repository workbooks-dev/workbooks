import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { ask } from "@tauri-apps/plugin-dialog";
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

  // Save tabs to localStorage whenever they change
  useEffect(() => {
    if (currentProject && tabs.length >= 0) {
      const projectKey = `tether_tabs_${currentProject.root}`;
      localStorage.setItem(projectKey, JSON.stringify({
        tabs: tabs,
        activeTabId: activeTabId
      }));
    }
  }, [tabs, activeTabId, currentProject]);

  // Prevent window close if there are unsaved changes
  useEffect(() => {
    const appWindow = getCurrentWindow();

    const unlisten = appWindow.onCloseRequested(async (event) => {
      // Check if any tabs have unsaved changes
      const hasUnsavedChanges = tabs.some(tab => tab.hasUnsavedChanges);

      if (hasUnsavedChanges) {
        // Prevent the close
        event.preventDefault();

        // Ask user what to do
        const shouldClose = await ask(
          "You have unsaved changes. Are you sure you want to close without saving?",
          {
            title: "Unsaved Changes",
            kind: "warning",
            okLabel: "Close Without Saving",
            cancelLabel: "Cancel",
          }
        );

        if (shouldClose) {
          // User confirmed, close the window
          await appWindow.destroy();
        }
      }
    });

    // Cleanup
    return () => {
      unlisten.then(fn => fn());
    };
  }, [tabs]);

  async function checkCurrentProject() {
    try {
      const project = await invoke("get_current_project");
      if (project) {
        setCurrentProject(project);
        restoreTabs(project);
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

  function restoreTabs(project) {
    const projectKey = `tether_tabs_${project.root}`;
    const savedState = localStorage.getItem(projectKey);

    if (savedState) {
      try {
        const { tabs: savedTabs, activeTabId: savedActiveTabId } = JSON.parse(savedState);
        if (savedTabs && savedTabs.length > 0) {
          setTabs(savedTabs);
          setActiveTabId(savedActiveTabId);
        }
      } catch (error) {
        console.error("Failed to restore tabs:", error);
      }
    }
  }

  function handleProjectOpened(project, mode) {
    if (mode === "create") {
      setView("create");
    } else if (project) {
      setCurrentProject(project);
      restoreTabs(project);
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
    <div className="flex flex-col h-screen w-screen bg-white">
      <div className="flex flex-1 overflow-hidden">
        <aside className="w-60 border-r border-gray-200 bg-gray-50 overflow-y-auto flex-shrink-0">
          <FileExplorer
            projectRoot={currentProject.root}
            projectName={currentProject.name}
            onOpenWorkbook={handleOpenFile}
            onFileDeleted={handleFileDeleted}
            activeFilePath={activeTab?.path}
          />
        </aside>
        <main className="flex-1 overflow-auto bg-white">
          <TabBar
            tabs={tabs}
            activeTabId={activeTabId}
            onTabSelect={handleTabSelect}
            onTabClose={handleTabClose}
            autosaveEnabled={autosaveEnabled}
            onAutosaveToggle={setAutosaveEnabled}
          />
          <div className="h-full">
            {activeTab ? (
              activeTab.type === "workbook" ? (
                <WorkbookViewer
                  key={activeTab.id}
                  workbookPath={activeTab.path}
                  projectRoot={currentProject.root}
                  autosaveEnabled={autosaveEnabled}
                  onClose={() => handleTabClose(activeTab.id)}
                  onUnsavedChangesUpdate={(hasChanges) => updateTabUnsavedState(activeTab.id, hasChanges)}
                />
              ) : (
                <FileViewer
                  key={activeTab.id}
                  filePath={activeTab.path}
                  onClose={() => handleTabClose(activeTab.id)}
                  onUnsavedChangesUpdate={(hasChanges) => updateTabUnsavedState(activeTab.id, hasChanges)}
                />
              )
            ) : (
              <div className="flex items-center justify-center h-full text-gray-400">
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
