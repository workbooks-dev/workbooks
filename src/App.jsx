import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { listen } from "@tauri-apps/api/event";
import { ask } from "@tauri-apps/plugin-dialog";
import { Welcome } from "./components/Welcome";
import { CreateProject } from "./components/CreateProject";
import { Sidebar } from "./components/Sidebar";
import { WorkbookViewer } from "./components/WorkbookViewer";
import { FileViewer } from "./components/FileViewer";
import { SecretsManager } from "./components/SecretsManager";
import { TabBar } from "./components/TabBar";
import { SaveConfirmDialog } from "./components/SaveConfirmDialog";
import "./App.css";

function App() {
  const [currentProject, setCurrentProject] = useState(null);
  const [loading, setLoading] = useState(true);
  const [view, setView] = useState("loading"); // loading, welcome, create, project
  const [tabs, setTabs] = useState([]);
  const [activeTabId, setActiveTabId] = useState(null);
  const [isDragging, setIsDragging] = useState(false);
  const [showSaveDialog, setShowSaveDialog] = useState(false);
  const [pendingClose, setPendingClose] = useState(null); // 'window' or 'tab'

  useEffect(() => {
    // Check if project path is provided via URL parameter
    const urlParams = new URLSearchParams(window.location.search);
    const projectPath = urlParams.get('project');

    if (projectPath) {
      // Load project from URL parameter
      loadProjectFromPath(projectPath);
    } else {
      // Check for current project
      checkCurrentProject();
    }
  }, []);

  // Listen for menu events from native menu
  useEffect(() => {
    let unlistenNewWindow;
    let unlistenShowLogs;
    let unlistenOpenLogsFolder;

    const setupMenuListeners = async () => {
      // Open new window
      unlistenNewWindow = await listen("menu:open-new-window", async () => {
        try {
          const { open } = await import("@tauri-apps/plugin-dialog");
          const folderPath = await open({
            directory: true,
            multiple: false,
            title: "Open Project in New Window",
          });

          if (folderPath) {
            await invoke("open_project_window", { projectPath: folderPath });
          }
        } catch (error) {
          console.error("Failed to open project in new window:", error);
        }
      });

      // Show logs in console
      unlistenShowLogs = await listen("menu:show-logs", async () => {
        try {
          const logs = await invoke("get_recent_logs", { lines: 1000 });
          console.log("=== TETHER RUNTIME LOGS ===");
          console.log(logs);
          console.log("=== END LOGS ===");
          alert("Logs have been written to the browser console. Open Developer Tools (Cmd+Option+I) to view them.");
        } catch (error) {
          console.error("Failed to get logs:", error);
          alert(`Failed to get logs: ${error}`);
        }
      });

      // Open logs folder
      unlistenOpenLogsFolder = await listen("menu:open-logs-folder", async () => {
        try {
          await invoke("open_logs_folder");
        } catch (error) {
          console.error("Failed to open logs folder:", error);
          alert(`Failed to open logs folder: ${error}`);
        }
      });
    };

    setupMenuListeners();

    return () => {
      if (unlistenNewWindow) unlistenNewWindow();
      if (unlistenShowLogs) unlistenShowLogs();
      if (unlistenOpenLogsFolder) unlistenOpenLogsFolder();
    };
  }, []);

  // Automatically install/update CLI on app launch
  useEffect(() => {
    const ensureCliUpToDate = async () => {
      try {
        // Get bundled CLI version
        const bundledVersion = await invoke("get_bundled_cli_version");

        // Check if CLI is installed
        const isInstalled = await invoke("check_cli_installed");

        if (!isInstalled) {
          // Not installed - install it
          console.log("CLI not found, installing version", bundledVersion);
          const result = await invoke("install_cli");
          console.log("✓ CLI installed:", result);

          // Get PATH instructions
          const instructions = await invoke("get_path_instructions");
          console.log("To use 'tether' from terminal:", instructions);
        } else {
          // Already installed - check if version matches
          const installedVersion = await invoke("get_installed_cli_version");

          if (installedVersion !== bundledVersion) {
            console.log(`CLI update available: ${installedVersion} → ${bundledVersion}`);
            const result = await invoke("install_cli");
            console.log("✓ CLI updated:", result);
          } else {
            console.log(`✓ CLI up to date (v${installedVersion})`);
          }
        }
      } catch (error) {
        // Silent failure - don't block app startup for CLI installation
        console.error("Failed to check/install CLI:", error);
      }
    };

    ensureCliUpToDate();
  }, []);

  // Listen for Tauri file drop events
  useEffect(() => {
    console.log("Setting up file drop listener, currentProject:", currentProject);

    let unlistenDrop;
    let unlistenHover;
    let unlistenCancel;

    const setupFileDropListener = async () => {
      try {
        console.log("Setting up file drop listeners...");

        // Listen for file drop
        unlistenDrop = await listen("tauri://drag-drop", async (event) => {
          console.log("!!! FILE DROP EVENT RECEIVED !!!", event);
          setIsDragging(false);

          if (!currentProject) {
            console.error("No current project");
            return;
          }

          const paths = event.payload.paths || event.payload;

          for (const filePath of paths) {
            try {
              // Handle the dropped item (file or folder) using Rust backend
              // This avoids needing frontend fs permissions - Rust handles everything
              const result = await invoke("handle_dropped_item", {
                projectRoot: currentProject.root,
                droppedPath: filePath,
              });
              console.log(`Successfully saved item at: ${result}`);
            } catch (error) {
              console.error(`Failed to save item:`, error);
              alert(`Failed to save item: ${error}`);
            }
          }

          // Refresh the file explorer
          console.log("Dispatching files-changed event");
          window.dispatchEvent(new CustomEvent("tether:files-changed"));
        });

        // Listen for file hover
        unlistenHover = await listen("tauri://drag-over", () => {
          console.log("Files hovering");
          setIsDragging(true);
        });

        // Listen for file drop cancelled
        unlistenCancel = await listen("tauri://drag-leave", () => {
          console.log("File drop cancelled");
          setIsDragging(false);
        });

        console.log("File drop listeners set up successfully");
      } catch (error) {
        console.error("Failed to set up file drop listeners:", error);
      }
    };

    if (currentProject) {
      setupFileDropListener();
    } else {
      console.log("No current project, skipping file drop listener setup");
    }

    return () => {
      console.log("Cleaning up file drop listeners");
      if (unlistenDrop) unlistenDrop();
      if (unlistenHover) unlistenHover();
      if (unlistenCancel) unlistenCancel();
    };
  }, [currentProject]);

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

        // Show custom save dialog
        setPendingClose('window');
        setShowSaveDialog(true);
      }
    });

    // Cleanup
    return () => {
      unlisten.then(fn => fn());
    };
  }, [tabs]);

  // Handle Command+W to close tabs or window
  useEffect(() => {
    if (view !== "project") return;

    const handleKeyDown = async (event) => {
      // Command+W on macOS or Ctrl+W on other platforms
      if ((event.metaKey || event.ctrlKey) && event.key === "w") {
        event.preventDefault();

        // If there's an active tab, close it
        if (activeTabId) {
          const tabToClose = tabs.find((tab) => tab.id === activeTabId);
          const isLastTab = tabs.length === 1;

          // Check if the tab has unsaved changes
          if (tabToClose?.hasUnsavedChanges) {
            // Show the save confirmation dialog
            setPendingClose(isLastTab ? 'window' : 'tab');
            setShowSaveDialog(true);
            return;
          }

          // No unsaved changes, just close
          handleTabClose(activeTabId);

          // If this was the last tab, close the window
          if (isLastTab) {
            const appWindow = getCurrentWindow();
            await appWindow.close();
          }
        } else if (tabs.length === 0) {
          // No tabs open, close the window
          const appWindow = getCurrentWindow();
          await appWindow.close();
        }
      }
    };

    window.addEventListener("keydown", handleKeyDown);

    return () => {
      window.removeEventListener("keydown", handleKeyDown);
    };
  }, [view, tabs, activeTabId]);

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

  async function loadProjectFromPath(projectPath) {
    try {
      const project = await invoke("load_project", { projectPath });
      setCurrentProject(project);
      restoreTabs(project);
      setView("project");
    } catch (error) {
      console.error("Failed to load project:", error);
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
      // For special tabs like secrets, use a friendly name
      name: fileType === 'secrets' ? 'Secrets' : undefined,
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

  // Handle save dialog actions
  const handleSaveAndClose = async () => {
    // Emit save event to all tabs with unsaved changes
    window.dispatchEvent(new CustomEvent("tether:save-all"));

    // Give tabs a moment to save (TODO: implement proper save confirmation)
    await new Promise(resolve => setTimeout(resolve, 500));

    setShowSaveDialog(false);

    if (pendingClose === 'window') {
      const appWindow = getCurrentWindow();
      await appWindow.destroy();
    } else if (pendingClose === 'tab') {
      // Close the active tab
      if (activeTabId) {
        handleTabClose(activeTabId);
      }
    }

    setPendingClose(null);
  };

  const handleDontSaveAndClose = async () => {
    setShowSaveDialog(false);

    if (pendingClose === 'window') {
      const appWindow = getCurrentWindow();
      await appWindow.destroy();
    } else if (pendingClose === 'tab') {
      // Close the active tab
      if (activeTabId) {
        handleTabClose(activeTabId);
      }
    }

    setPendingClose(null);
  };

  const handleCancelClose = () => {
    setShowSaveDialog(false);
    setPendingClose(null);
  };


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
      <div className="min-h-screen bg-gradient-to-br from-gray-50 to-gray-100">
        <div className="fixed top-0 left-0 right-0 bg-white border-b border-gray-200 px-6 py-4 flex items-center justify-between shadow-sm">
          <h1 className="text-xl font-semibold text-gray-900">Tether</h1>
          <button
            onClick={() => setView("welcome")}
            className="px-4 py-2 text-sm font-medium text-gray-700 bg-white border border-gray-300 rounded-md hover:bg-gray-50"
          >
            Back
          </button>
        </div>
        <div className="pt-20">
          <CreateProject onProjectCreated={handleProjectCreated} />
        </div>
      </div>
    );
  }

  const activeTab = tabs.find((tab) => tab.id === activeTabId);

  return (
    <div className="flex flex-col h-screen w-screen bg-white relative">
      {isDragging && (
        <div className="absolute inset-0 bg-blue-500 bg-opacity-10 border-4 border-blue-500 border-dashed z-50 flex items-center justify-center pointer-events-none">
          <div className="bg-white px-6 py-4 rounded-lg shadow-lg">
            <p className="text-lg font-medium text-gray-700">Drop files here</p>
            <p className="text-sm text-gray-500 mt-1">.ipynb files → /notebooks, others → project root</p>
          </div>
        </div>
      )}

      <div className="flex flex-1 overflow-hidden">
        <aside className="w-64 border-r border-gray-200 bg-gray-50 overflow-y-auto flex-shrink-0">
          <Sidebar
            projectRoot={currentProject.root}
            projectName={currentProject.name}
            onOpenFile={handleOpenFile}
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
          />
          <div className="h-full">
            {activeTab ? (
              activeTab.type === "workbook" ? (
                <WorkbookViewer
                  key={activeTab.id}
                  workbookPath={activeTab.path}
                  projectRoot={currentProject.root}
                  onClose={() => handleTabClose(activeTab.id)}
                  onUnsavedChangesUpdate={(hasChanges) => updateTabUnsavedState(activeTab.id, hasChanges)}
                />
              ) : activeTab.type === "secrets" ? (
                <SecretsManager
                  key={activeTab.id}
                  projectRoot={currentProject.root}
                  onClose={() => handleTabClose(activeTab.id)}
                />
              ) : (
                <FileViewer
                  key={activeTab.id}
                  filePath={activeTab.path}
                  projectRoot={currentProject.root}
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

      {/* Save confirmation dialog */}
      <SaveConfirmDialog
        isOpen={showSaveDialog}
        onSave={handleSaveAndClose}
        onDontSave={handleDontSaveAndClose}
        onCancel={handleCancelClose}
        message="You have unsaved changes. Would you like to save before closing?"
      />
    </div>
  );
}

export default App;
