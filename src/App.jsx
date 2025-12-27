import { useState, useEffect, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { listen } from "@tauri-apps/api/event";
import { getWindowScreenshot, getScreenshotableWindows } from "tauri-plugin-screenshots-api";
import { writeImage } from "@tauri-apps/plugin-clipboard-manager";
import { ActionWindow } from "./components/ActionWindow";
import { Welcome } from "./components/Welcome";
import { CreateProject } from "./components/CreateProject";
import { Sidebar } from "./components/Sidebar";
import { AiChatPanel } from "./components/AiChatPanel";
import { WorkbookViewer } from "./components/WorkbookViewer";
import { FileViewer } from "./components/FileViewer";
import { SecretsManager } from "./components/SecretsManager";
import { ScheduleTab } from "./components/ScheduleTab";
import { AppSettings } from "./components/AppSettings";
import { TabBar } from "./components/TabBar";
import { SaveConfirmDialog } from "./components/SaveConfirmDialog";
import { ResizablePanel } from "./components/ResizablePanel";
import "./App.css";

function App() {
  const [currentProject, setCurrentProject] = useState(null);
  const [loading, setLoading] = useState(true);
  const [view, setView] = useState("loading"); // loading, action, welcome, create, project, global-schedules, global-runs
  const [tabs, setTabs] = useState([]);
  const [activeTabId, setActiveTabId] = useState(null);
  const [isDragging, setIsDragging] = useState(false);
  const [showSaveDialog, setShowSaveDialog] = useState(false);
  const [pendingClose, setPendingClose] = useState(null); // 'window' or 'tab'
  const [aiEnabled, setAiEnabled] = useState(false);
  const [initialChatSession, setInitialChatSession] = useState(null);

  // Panel visibility state
  const [showLeftSidebar, setShowLeftSidebar] = useState(() => {
    const saved = localStorage.getItem('workbooks_show_left_sidebar');
    return saved === null ? true : saved === 'true';
  });
  const [showAiChat, setShowAiChat] = useState(() => {
    const saved = localStorage.getItem('workbooks_show_ai_chat');
    return saved === null ? true : saved === 'true';
  });
  const [showRightPanel, setShowRightPanel] = useState(() => {
    const saved = localStorage.getItem('workbooks_show_right_panel');
    return saved === null ? true : saved === 'true';
  });

  // WorkbookViewer ref - used to trigger inline diff when AI makes changes
  const workbookViewerRef = useRef(null);

  useEffect(() => {
    // Check URL parameters for initial view/project
    const urlParams = new URLSearchParams(window.location.search);
    const projectPath = urlParams.get('project');
    const viewParam = urlParams.get('view');

    if (projectPath) {
      // Load project from URL parameter
      loadProjectFromPath(projectPath);
    } else if (viewParam) {
      // Navigate to specific view (global-runs, global-schedules, create, action, etc.)
      console.log("Navigating to view from URL:", viewParam);
      setView(viewParam);
      setLoading(false);
    } else {
      // Check for current project
      checkCurrentProject();
    }

    // Load global config to check if AI features are enabled
    loadAiConfig();
  }, []);

  async function loadAiConfig() {
    try {
      const config = await invoke("get_global_config");
      setAiEnabled(config.ai?.enabled || false);
    } catch (error) {
      console.error("Failed to load AI config:", error);
    }
  }

  // Listen for menu events from native menu
  useEffect(() => {
    let unlistenNewWindow;
    let unlistenShowLogs;
    let unlistenOpenLogsFolder;
    let unlistenTakeScreenshot;
    let unlistenSettings;

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
          console.log("=== WORKBOOKS RUNTIME LOGS ===");
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

      // Take screenshot
      unlistenTakeScreenshot = await listen("menu:take-screenshot", async () => {
        try {
          // Get all screenshotable windows
          const windows = await getScreenshotableWindows();
          console.log("Available windows:", windows);

          // Find the Workbooks window
          const workbooksWindow = windows.find(w =>
            w.title?.toLowerCase().includes("workbooks") ||
            w.appName?.toLowerCase().includes("workbooks")
          );

          if (!workbooksWindow) {
            alert("Could not find Workbooks window. Available windows: " + windows.map(w => w.title).join(", "));
            return;
          }

          console.log("Taking screenshot of window:", workbooksWindow);

          // Take screenshot
          const screenshot = await getWindowScreenshot(workbooksWindow);

          // Copy to clipboard
          await writeImage(screenshot);

          console.log("Screenshot copied to clipboard!");
          alert("Screenshot copied to clipboard!");

        } catch (error) {
          console.error("Failed to take screenshot:", error);
          alert(`Failed to take screenshot: ${error.message || error}`);
        }
      });

      // Settings
      unlistenSettings = await listen("menu:settings", async () => {
        console.log("Settings menu clicked");
        // Navigate to settings (works with or without a project)
        if (currentProject) {
          // If in project view, open settings as a tab
          handleOpenSettings();
        } else {
          // If not in project view, navigate to settings view
          setView("settings");
        }
      });
    };

    setupMenuListeners();

    return () => {
      if (unlistenNewWindow) unlistenNewWindow();
      if (unlistenShowLogs) unlistenShowLogs();
      if (unlistenOpenLogsFolder) unlistenOpenLogsFolder();
      if (unlistenTakeScreenshot) unlistenTakeScreenshot();
      if (unlistenSettings) unlistenSettings();
    };
  }, []);

  // Listen for tray menu events
  useEffect(() => {
    let unlistenOpenProject;
    let unlistenCreateProject;
    let unlistenOpenProjectDialog;
    let unlistenViewRuns;
    let unlistenViewScheduler;
    let unlistenSettings;

    const setupTrayListeners = async () => {
      console.log("Setting up tray listeners...");

      // Handle recent project clicks from tray
      unlistenOpenProject = await listen("open-project", async (event) => {
        console.log("Received open-project event:", event.payload);
        try {
          const { path } = event.payload;
          await loadProjectFromPath(path);
        } catch (error) {
          console.error("Failed to open project from tray:", error);
        }
      });

      // Create new project
      unlistenCreateProject = await listen("tray-create-project", async () => {
        console.log("Received tray-create-project event");
        // Clear project state and navigate to create view
        setCurrentProject(null);
        setTabs([]);
        setActiveTabId(null);
        setView("create");
      });

      // Open project dialog
      unlistenOpenProjectDialog = await listen("tray-open-project", async () => {
        console.log("Received tray-open-project event");
        try {
          const { open } = await import("@tauri-apps/plugin-dialog");
          const folderPath = await open({
            directory: true,
            multiple: false,
            title: "Open Project",
          });

          if (folderPath) {
            await loadProjectFromPath(folderPath);
          } else {
            // User cancelled - go to action window
            setCurrentProject(null);
            setTabs([]);
            setActiveTabId(null);
            setView("action");
          }
        } catch (error) {
          console.error("Failed to open project:", error);
        }
      });

      // View runs
      unlistenViewRuns = await listen("tray-view-runs", async () => {
        console.log("Received tray-view-runs event");
        // Clear project state and navigate to global runs
        setCurrentProject(null);
        setTabs([]);
        setActiveTabId(null);
        setView("global-runs");
      });

      // View scheduler
      unlistenViewScheduler = await listen("tray-view-scheduler", async () => {
        console.log("Received tray-view-scheduler event");
        // Clear project state and navigate to global schedules
        setCurrentProject(null);
        setTabs([]);
        setActiveTabId(null);
        setView("global-schedules");
      });

      // Settings
      unlistenSettings = await listen("tray-settings", async () => {
        console.log("Received tray-settings event");
        // Clear project state and navigate to settings
        setCurrentProject(null);
        setTabs([]);
        setActiveTabId(null);
        setView("settings");
      });

      console.log("Tray listeners set up successfully");
    };

    setupTrayListeners();

    return () => {
      console.log("Cleaning up tray listeners");
      if (unlistenOpenProject) unlistenOpenProject();
      if (unlistenCreateProject) unlistenCreateProject();
      if (unlistenOpenProjectDialog) unlistenOpenProjectDialog();
      if (unlistenViewRuns) unlistenViewRuns();
      if (unlistenViewScheduler) unlistenViewScheduler();
      if (unlistenSettings) unlistenSettings();
    };
  }, []); // Remove dependencies so listeners stay active

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
          console.log("To use 'workbooks' from terminal:", instructions);
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
          window.dispatchEvent(new CustomEvent("workbooks:files-changed"));
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
      const projectKey = `workbooks_tabs_${currentProject.root}`;
      localStorage.setItem(projectKey, JSON.stringify({
        tabs: tabs,
        activeTabId: activeTabId
      }));
    }
  }, [tabs, activeTabId, currentProject]);

  // Handle window close requests
  useEffect(() => {
    const appWindow = getCurrentWindow();

    const unlisten = appWindow.onCloseRequested(async (event) => {
      // If in Action Window or global views, allow close (will hide window via lib.rs)
      if (view === "action" || view === "global-schedules" || view === "global-runs") {
        return; // Allow close
      }

      // If in create or project view, prevent close and reset to Action Window
      event.preventDefault();

      // Check if any tabs have unsaved changes
      const hasUnsavedChanges = tabs.some(tab => tab.hasUnsavedChanges);

      if (hasUnsavedChanges) {
        // Show custom save dialog, then reset to Action Window
        setPendingClose('reset-to-action');
        setShowSaveDialog(true);
      } else {
        // No unsaved changes, just reset to Action Window
        setCurrentProject(null);
        setTabs([]);
        setActiveTabId(null);
        setView("action");
      }
    });

    // Cleanup
    return () => {
      unlisten.then(fn => fn());
    };
  }, [view, tabs]);

  // Save panel visibility to localStorage
  useEffect(() => {
    localStorage.setItem('workbooks_show_left_sidebar', showLeftSidebar.toString());
  }, [showLeftSidebar]);

  useEffect(() => {
    localStorage.setItem('workbooks_show_ai_chat', showAiChat.toString());
  }, [showAiChat]);

  useEffect(() => {
    localStorage.setItem('workbooks_show_right_panel', showRightPanel.toString());
  }, [showRightPanel]);

  // Handle keyboard shortcuts for panels and tabs
  useEffect(() => {
    const handleKeyDown = async (event) => {
      // Cmd+B (or Ctrl+B) - Toggle left sidebar (VS Code standard)
      if ((event.metaKey || event.ctrlKey) && event.key === "b" && !event.shiftKey) {
        event.preventDefault();
        setShowLeftSidebar(prev => !prev);
        return;
      }

      // Cmd+J (or Ctrl+J) - Toggle AI chat panel (VS Code standard for bottom panel)
      if ((event.metaKey || event.ctrlKey) && event.key === "j" && !event.shiftKey) {
        event.preventDefault();
        setShowAiChat(prev => !prev);
        return;
      }

      // Cmd+Shift+B (or Ctrl+Shift+B) - Toggle right panel (file viewer)
      if ((event.metaKey || event.ctrlKey) && event.shiftKey && event.key === "B") {
        event.preventDefault();
        setShowRightPanel(prev => !prev);
        return;
      }

      // Command+W on macOS or Ctrl+W on other platforms - Close tab
      if ((event.metaKey || event.ctrlKey) && event.key === "w") {
        event.preventDefault();

        // If in Action Window or global views, hide the window
        if (view === "action" || view === "global-schedules" || view === "global-runs") {
          const appWindow = getCurrentWindow();
          await appWindow.close(); // Will hide via lib.rs
          return;
        }

        // If in create view, reset to Action Window
        if (view === "create") {
          setView("action");
          return;
        }

        // If in project view, handle tab closing
        if (view === "project") {
          // If there's an active tab, close it
          if (activeTabId) {
            const tabToClose = tabs.find((tab) => tab.id === activeTabId);
            const isLastTab = tabs.length === 1;

            // Check if the tab has unsaved changes
            if (tabToClose?.hasUnsavedChanges) {
              // Show the save confirmation dialog
              setPendingClose(isLastTab ? 'reset-to-action' : 'tab');
              setShowSaveDialog(true);
              return;
            }

            // No unsaved changes, just close
            handleTabClose(activeTabId);

            // If this was the last tab, reset to Action Window
            if (isLastTab) {
              setCurrentProject(null);
              setTabs([]);
              setActiveTabId(null);
              setView("action");
            }
          } else if (tabs.length === 0) {
            // No tabs open, reset to Action Window
            setCurrentProject(null);
            setView("action");
          }
        }
      }
    };

    window.addEventListener("keydown", handleKeyDown);

    return () => {
      window.removeEventListener("keydown", handleKeyDown);
    };
  }, [view, tabs, activeTabId, showLeftSidebar, showAiChat, showRightPanel]);

  async function checkCurrentProject() {
    try {
      const project = await invoke("get_current_project");
      if (project) {
        setCurrentProject(project);
        restoreTabs(project);
        setView("project");
      } else {
        setView("action");
      }
    } catch (error) {
      console.error("Failed to get current project:", error);
      setView("action");
    } finally {
      setLoading(false);
    }
  }

  async function loadProjectFromPath(projectPath) {
    try {
      // Use open_folder instead of load_project to ensure project is initialized
      const project = await invoke("open_folder", { folderPath: projectPath });

      // Initialize Python environment and sync dependencies
      console.log("Initializing Python environment...");
      await invoke("init_python_env", {
        projectPath: project.root,
      });
      console.log("Python environment initialized");

      // Get or create a chat session for this project
      try {
        const chatSession = await invoke("get_or_create_project_chat_session", {
          projectRoot: project.root,
          projectName: project.name,
        });
        console.log("Loaded chat session for project:", chatSession);
        setInitialChatSession(chatSession);
      } catch (error) {
        console.error("Failed to get/create project chat session:", error);
      }

      setCurrentProject(project);
      restoreTabs(project);
      setView("project");
    } catch (error) {
      console.error("Failed to load project:", error);
      setView("action");
    } finally {
      setLoading(false);
    }
  }

  function restoreTabs(project) {
    const projectKey = `workbooks_tabs_${project.root}`;
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
      // For special tabs like secrets and schedule, use a friendly name
      name: fileType === 'secrets' ? 'Secrets' : fileType === 'schedule' ? 'Schedule' : undefined,
    };

    setTabs([...tabs, newTab]);
    setActiveTabId(newTab.id);
  }

  function handleOpenSettings() {
    // Check if settings tab is already open
    const existingTab = tabs.find((tab) => tab.type === 'settings');
    if (existingTab) {
      setActiveTabId(existingTab.id);
      return;
    }

    // Create new settings tab
    const newTab = {
      id: Date.now().toString(),
      path: 'settings',
      type: 'settings',
      hasUnsavedChanges: false,
      name: 'Settings',
    };

    setTabs([...tabs, newTab]);
    setActiveTabId(newTab.id);
  }

  // === Notebook Change Approval Handlers ===

  /**
   * Request user approval for AI-generated notebook changes (inline diff)
   * @param {string} filePath - Path to the notebook being modified
   * @param {object} oldNotebook - Previous notebook content (parsed JSON)
   * @param {object} newNotebook - New notebook content (parsed JSON)
   */
  async function handleRequestNotebookApproval(filePath, oldNotebook, newNotebook) {
    // First, open the notebook if it's not already open
    const existingTab = tabs.find(t => t.path === filePath && t.type === "workbook");

    if (!existingTab) {
      // Open the notebook
      handleOpenFile(filePath, "workbook");

      // Wait a bit for the component to mount and ref to be attached
      setTimeout(() => {
        if (workbookViewerRef.current) {
          workbookViewerRef.current.handleAiChanges(oldNotebook, newNotebook);
        }
      }, 100);
    } else {
      // Notebook is already open, activate it and trigger diff
      setActiveTabId(existingTab.id);

      // Small delay to ensure the WorkbookViewer is rendered and ref is available
      setTimeout(() => {
        if (workbookViewerRef.current) {
          workbookViewerRef.current.handleAiChanges(oldNotebook, newNotebook);
        }
      }, 50);
    }
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

  function handleFileRenamed(oldPath, newPath) {
    // Update tabs that have this file path
    setTabs((prevTabs) =>
      prevTabs.map((tab) =>
        tab.path === oldPath ? { ...tab, path: newPath } : tab
      )
    );
  }

  function handleFileDeleted(filePath) {
    // Mark tabs as deleted instead of closing them
    // This allows users to re-save if needed
    setTabs((prevTabs) =>
      prevTabs.map((tab) =>
        tab.path === filePath ? { ...tab, isDeleted: true } : tab
      )
    );
  }

  function handleFileRestored(filePath) {
    // Clear the isDeleted flag when file is restored
    setTabs((prevTabs) =>
      prevTabs.map((tab) =>
        tab.path === filePath ? { ...tab, isDeleted: false } : tab
      )
    );
  }

  // Handle save dialog actions
  const handleSaveAndClose = async () => {
    // Emit save event to all tabs with unsaved changes
    window.dispatchEvent(new CustomEvent("workbooks:save-all"));

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
    } else if (pendingClose === 'reset-to-action') {
      // Reset to Action Window
      setCurrentProject(null);
      setTabs([]);
      setActiveTabId(null);
      setView("action");
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
    } else if (pendingClose === 'reset-to-action') {
      // Reset to Action Window without saving
      setCurrentProject(null);
      setTabs([]);
      setActiveTabId(null);
      setView("action");
    }

    setPendingClose(null);
  };

  const handleCancelClose = () => {
    setShowSaveDialog(false);
    setPendingClose(null);
  };

  function handleActionWindowAction(action) {
    switch (action.type) {
      case "create-project":
        setView("create");
        break;
      case "open-project":
        loadProjectFromPath(action.path);
        break;
      case "view-all-runs":
        setView("global-runs");
        break;
      case "view-all-schedules":
        setView("global-schedules");
        break;
      case "open-settings":
        setView("settings");
        break;
      default:
        console.warn("Unknown action:", action);
    }
  }


  if (loading || view === "loading") {
    return (
      <div className="app loading">
        <p>Loading...</p>
      </div>
    );
  }

  if (view === "action") {
    return (
      <div className="app">
        <ActionWindow onAction={handleActionWindowAction} />
      </div>
    );
  }

  if (view === "welcome") {
    return (
      <div className="app welcome-view">
        <Welcome
          onProjectOpened={handleProjectOpened}
          onOpenSettings={() => setView("settings")}
        />
      </div>
    );
  }

  // Settings view (accessible without a project)
  if (view === "settings") {
    return (
      <div className="app settings-view">
        <AppSettings onClose={() => setView("action")} />
      </div>
    );
  }

  if (view === "create") {
    return (
      <div className="min-h-screen bg-gradient-to-br from-gray-50 to-gray-100">
        <div className="fixed top-0 left-0 right-0 bg-white border-b border-gray-200 px-6 py-4 flex items-center justify-between shadow-sm z-10">
          <h1 className="text-xl font-semibold text-gray-900">Workbooks</h1>
          <button
            onClick={() => setView("action")}
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

  if (view === "global-schedules") {
    return (
      <div className="min-h-screen bg-gray-50">
        <div className="fixed top-0 left-0 right-0 bg-white border-b border-gray-200 px-6 py-4 flex items-center justify-between shadow-sm z-10">
          <h1 className="text-xl font-semibold text-gray-900">All Schedules</h1>
          <button
            onClick={() => setView("action")}
            className="px-4 py-2 text-sm font-medium text-gray-700 bg-white border border-gray-300 rounded-md hover:bg-gray-50"
          >
            Back
          </button>
        </div>
        <div className="pt-20 h-screen">
          <ScheduleTab
            key="global-schedules"
            projectRoot={null}
            initialSubTab="scheduled"
            initialShowAllProjects={true}
            onClose={() => setView("action")}
          />
        </div>
      </div>
    );
  }

  if (view === "global-runs") {
    return (
      <div className="min-h-screen bg-gray-50">
        <div className="fixed top-0 left-0 right-0 bg-white border-b border-gray-200 px-6 py-4 flex items-center justify-between shadow-sm z-10">
          <h1 className="text-xl font-semibold text-gray-900">All Runs</h1>
          <button
            onClick={() => setView("action")}
            className="px-4 py-2 text-sm font-medium text-gray-700 bg-white border border-gray-300 rounded-md hover:bg-gray-50"
          >
            Back
          </button>
        </div>
        <div className="pt-20 h-screen">
          <ScheduleTab
            key="global-runs"
            projectRoot={null}
            initialSubTab="runs"
            initialShowAllProjects={true}
            onClose={() => setView("action")}
          />
        </div>
      </div>
    );
  }

  const activeTab = tabs.find((tab) => tab.id === activeTabId);

  // Prepare focused file info for AI
  const focusedFile = activeTab ? {
    path: activeTab.path,
    name: activeTab.name || activeTab.path.split('/').pop(),
    type: activeTab.type
  } : null;

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

      {/* Top Bar with Panel Toggles (VS Code style) */}
      <div className="flex items-center justify-end gap-1 px-3 py-1.5 border-b border-gray-200 bg-white">
        <button
          onClick={() => setShowLeftSidebar(!showLeftSidebar)}
          className={`p-1.5 hover:bg-gray-100 transition-colors ${showLeftSidebar ? 'text-blue-600' : 'text-gray-600'}`}
          title="Toggle Primary Sidebar (⌘B)"
        >
          <svg className="w-4 h-4" fill="currentColor" viewBox="0 0 16 16">
            <path d="M14 2H2a1 1 0 0 0-1 1v10a1 1 0 0 0 1 1h12a1 1 0 0 0 1-1V3a1 1 0 0 0-1-1zM2 3h3v10H2V3zm4 0h8v10H6V3z"/>
          </svg>
        </button>
        <button
          onClick={() => setShowAiChat(!showAiChat)}
          className={`p-1.5 hover:bg-gray-100 transition-colors ${showAiChat ? 'text-blue-600' : 'text-gray-600'}`}
          title="Toggle Panel (⌘J)"
        >
          <svg className="w-4 h-4" fill="currentColor" viewBox="0 0 16 16">
            <path d="M14 2H2a1 1 0 0 0-1 1v10a1 1 0 0 0 1 1h12a1 1 0 0 0 1-1V3a1 1 0 0 0-1-1zM2 3h12v7H2V3zm0 8h12v2H2v-2z"/>
          </svg>
        </button>
        <button
          onClick={() => setShowRightPanel(!showRightPanel)}
          className={`p-1.5 hover:bg-gray-100 transition-colors ${showRightPanel ? 'text-blue-600' : 'text-gray-600'}`}
          title="Toggle Secondary Sidebar (⌘⇧B)"
        >
          <svg className="w-4 h-4" fill="currentColor" viewBox="0 0 16 16">
            <path d="M14 2H2a1 1 0 0 0-1 1v10a1 1 0 0 0 1 1h12a1 1 0 0 0 1-1V3a1 1 0 0 0-1-1zM2 3h8v10H2V3zm9 0h3v10h-3V3z"/>
          </svg>
        </button>
      </div>

      <div className="flex flex-1 overflow-hidden">
        {/* Left Sidebar */}
        <ResizablePanel
          isCollapsed={!showLeftSidebar}
          defaultWidth={256}
          minWidth={200}
          maxWidth={500}
          side="right"
          storageKey="workbooks_left_sidebar_width"
          className="border-r border-gray-200 bg-gray-50"
        >
          <div className="h-full overflow-y-auto">
            <Sidebar
              projectRoot={currentProject.root}
              projectName={currentProject.name}
              onOpenFile={handleOpenFile}
              onFileDeleted={handleFileDeleted}
              onFileRenamed={handleFileRenamed}
              onOpenSettings={handleOpenSettings}
              activeFilePath={activeTab?.path}
            />
          </div>
        </ResizablePanel>

        {/* Main Content Area */}
        <main className="flex-1 overflow-hidden bg-white flex flex-col min-w-0">
          {/* Tab Bar - only show when there are tabs */}
          {tabs.length > 0 && (
            <div className="flex items-center border-b border-gray-200 bg-white">
              <div className="flex-1">
                <TabBar
                  tabs={tabs}
                  activeTabId={activeTabId}
                  onTabSelect={handleTabSelect}
                  onTabClose={handleTabClose}
                />
              </div>
            </div>
          )}

          {/* Content Split View */}
          <div className="flex-1 flex overflow-hidden">
            {/* AI Chat Panel */}
            {showAiChat && (
              <>
                {activeTab ? (
                  <ResizablePanel
                    defaultWidth={500}
                    minWidth={300}
                    maxWidth={1200}
                    side="right"
                    storageKey="workbooks_ai_chat_width"
                    className="border-r border-gray-200 overflow-hidden"
                  >
                    <AiChatPanel
                      projectRoot={currentProject.root}
                      aiEnabled={aiEnabled}
                      onOpenSettings={handleOpenSettings}
                      focusedFile={focusedFile}
                      onOpenFile={handleOpenFile}
                      onRequestNotebookApproval={handleRequestNotebookApproval}
                      initialSession={initialChatSession}
                    />
                  </ResizablePanel>
                ) : (
                  <div className="flex-1 overflow-hidden">
                    <AiChatPanel
                      projectRoot={currentProject.root}
                      aiEnabled={aiEnabled}
                      onOpenSettings={handleOpenSettings}
                      focusedFile={focusedFile}
                      onOpenFile={handleOpenFile}
                      onRequestNotebookApproval={handleRequestNotebookApproval}
                      initialSession={initialChatSession}
                    />
                  </div>
                )}
              </>
            )}

            {/* File Viewer - Only when a tab is active and right panel is visible */}
            {activeTab && showRightPanel && (
              <div className="flex-1 overflow-auto">
                {activeTab.type === "workbook" ? (
                  <WorkbookViewer
                    ref={workbookViewerRef}
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
                ) : activeTab.type === "schedule" ? (
                  <ScheduleTab
                    key={activeTab.id}
                    projectRoot={currentProject.root}
                    onClose={() => handleTabClose(activeTab.id)}
                  />
                ) : activeTab.type === "settings" ? (
                  <AppSettings
                    key={activeTab.id}
                    onClose={() => handleTabClose(activeTab.id)}
                  />
                ) : (
                  <FileViewer
                    key={activeTab.id}
                    filePath={activeTab.path}
                    projectRoot={currentProject.root}
                    isDeleted={activeTab.isDeleted}
                    onClose={() => handleTabClose(activeTab.id)}
                    onUnsavedChangesUpdate={(hasChanges) => updateTabUnsavedState(activeTab.id, hasChanges)}
                    onFileRestored={() => handleFileRestored(activeTab.path)}
                  />
                )}
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
