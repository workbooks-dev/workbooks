import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { ask } from "@tauri-apps/plugin-dialog";
import { ContextMenu } from "./ContextMenu";
import { InputDialog } from "./InputDialog";
import { WorkbooksTableView } from "./WorkbooksTableView";

// Collapsible section component
function SidebarSection({ title, icon, children, defaultExpanded = true, onHeaderClick }) {
  const [isExpanded, setIsExpanded] = useState(defaultExpanded);

  const handleHeaderClick = () => {
    if (onHeaderClick) {
      onHeaderClick();
    } else {
      setIsExpanded(!isExpanded);
    }
  };

  return (
    <div className="border-b border-gray-200">
      <button
        onClick={handleHeaderClick}
        className="w-full px-4 py-3 flex items-center justify-between hover:bg-gray-100 transition-colors"
      >
        <div className="flex items-center gap-2">
          <span className="text-sm">{icon}</span>
          <h3 className="text-xs font-semibold uppercase tracking-wider text-gray-700">
            {title}
          </h3>
        </div>
        {!onHeaderClick && (
          <span className="text-xs text-gray-400">
            {isExpanded ? "▼" : "▶"}
          </span>
        )}
      </button>
      {isExpanded && <div className="pb-2">{children}</div>}
    </div>
  );
}

// File tree item for Files section (filters out .ipynb)
function FileTreeItem({ file, level = 0, onFileClick, onFileAction, activeFilePath }) {
  const [isExpanded, setIsExpanded] = useState(false);
  const [children, setChildren] = useState([]);
  const [loading, setLoading] = useState(false);
  const [showContextMenu, setShowContextMenu] = useState(false);
  const [contextMenuPos, setContextMenuPos] = useState({ x: 0, y: 0 });

  const handleToggle = async () => {
    if (file.is_dir) {
      if (!isExpanded) {
        setLoading(true);
        try {
          const fileList = await invoke("list_files", {
            directoryPath: file.path,
          });
          // Filter out .ipynb files in Files section
          const nonNotebookFiles = fileList.filter(f => f.extension !== "ipynb");
          setChildren(nonNotebookFiles);
        } catch (err) {
          console.error("Failed to load folder contents:", err);
        } finally {
          setLoading(false);
        }
      }
      setIsExpanded(!isExpanded);
    } else {
      onFileClick?.(file);
    }
  };

  const handleContextMenu = (e) => {
    e.preventDefault();
    e.stopPropagation();
    setContextMenuPos({ x: e.clientX, y: e.clientY });
    setShowContextMenu(true);
  };

  const getFileIcon = () => {
    if (file.is_dir) {
      return isExpanded ? "▼" : "▶";
    }

    switch (file.extension) {
      case "py":
        return "🐍";
      case "md":
        return "📝";
      case "json":
      case "toml":
      case "yaml":
      case "yml":
        return "⚙️";
      case "csv":
        return "📊";
      case "txt":
        return "📄";
      default:
        return "📄";
    }
  };

  const getContextMenuItems = () => {
    return [
      { label: "Rename", action: () => onFileAction('rename', file) },
      { label: "Delete", action: () => onFileAction('delete', file) }
    ];
  };

  const isActive = activeFilePath === file.path;

  return (
    <>
      <div
        className={`flex items-center gap-2 px-3 py-1.5 cursor-pointer text-sm rounded mx-2 my-0.5 transition-all ${
          isActive
            ? 'bg-blue-50 text-blue-900 font-medium border-l-2 border-blue-500'
            : 'text-gray-700 hover:bg-white'
        }`}
        style={{ paddingLeft: `${level * 12 + 12}px` }}
        onClick={handleToggle}
        onContextMenu={handleContextMenu}
      >
        <span className="text-xs opacity-60 w-4 text-center flex-shrink-0">{getFileIcon()}</span>
        <span className="flex-1 overflow-hidden text-ellipsis whitespace-nowrap">{file.name}</span>
        {loading && <span className="text-xs text-gray-400">...</span>}
      </div>
      {showContextMenu && (
        <ContextMenu
          x={contextMenuPos.x}
          y={contextMenuPos.y}
          items={getContextMenuItems()}
          onClose={() => setShowContextMenu(false)}
        />
      )}
      {isExpanded && children.length > 0 && (
        <div>
          {children.map((child) => (
            <FileTreeItem
              key={child.path}
              file={child}
              level={level + 1}
              onFileClick={onFileClick}
              onFileAction={onFileAction}
              activeFilePath={activeFilePath}
            />
          ))}
        </div>
      )}
    </>
  );
}

export function Sidebar({ projectRoot, projectName, onOpenFile, onFileDeleted, activeFilePath }) {
  const [files, setFiles] = useState([]);
  const [workbooks, setWorkbooks] = useState([]);
  const [loading, setLoading] = useState(false);
  const [creatingWorkbook, setCreatingWorkbook] = useState(false);
  const [workbookName, setWorkbookName] = useState("");
  const [showWorkbooksTable, setShowWorkbooksTable] = useState(false);
  const [renamingFile, setRenamingFile] = useState(null);
  const [recentWorkbooks, setRecentWorkbooks] = useState([]);

  useEffect(() => {
    loadProjectFiles();
    loadRecentWorkbooks();

    // Listen for file changes (e.g., when files are dropped)
    const handleFilesChanged = () => {
      loadProjectFiles();
    };

    window.addEventListener("tether:files-changed", handleFilesChanged);

    return () => {
      window.removeEventListener("tether:files-changed", handleFilesChanged);
    };
  }, [projectRoot]);

  const loadProjectFiles = async () => {
    setLoading(true);
    try {
      const fileList = await invoke("list_files", {
        directoryPath: projectRoot,
      });

      // Separate workbooks from other files
      const notebookFiles = fileList.filter(f => f.extension === "ipynb" || f.name === "notebooks");
      const otherFiles = fileList.filter(f => f.extension !== "ipynb" && f.name !== "notebooks");

      setFiles(otherFiles);

      // Load workbooks from notebooks folder if it exists
      const notebooksFolder = fileList.find(f => f.is_dir && f.name === "notebooks");
      if (notebooksFolder) {
        const notebooksList = await invoke("list_files", {
          directoryPath: notebooksFolder.path,
        });
        setWorkbooks(notebooksList.filter(f => f.extension === "ipynb"));
      } else {
        setWorkbooks([]);
      }
    } catch (err) {
      console.error("Failed to load files:", err);
    } finally {
      setLoading(false);
    }
  };

  const loadRecentWorkbooks = () => {
    // Load recent workbooks from localStorage
    const projectKey = `tether_recent_workbooks_${projectRoot}`;
    const recent = localStorage.getItem(projectKey);
    if (recent) {
      try {
        setRecentWorkbooks(JSON.parse(recent));
      } catch (err) {
        console.error("Failed to load recent workbooks:", err);
        setRecentWorkbooks([]);
      }
    }
  };

  const updateRecentWorkbook = (workbookPath) => {
    // Update recent workbooks list
    const projectKey = `tether_recent_workbooks_${projectRoot}`;
    const recent = recentWorkbooks.filter(path => path !== workbookPath);
    recent.unshift(workbookPath); // Add to front
    const trimmed = recent.slice(0, 20); // Keep only 20 most recent
    setRecentWorkbooks(trimmed);
    localStorage.setItem(projectKey, JSON.stringify(trimmed));
  };

  const handleFileClick = (file) => {
    if (file.extension === "ipynb") {
      onOpenFile?.(file.path, "workbook");
      updateRecentWorkbook(file.path);
    } else {
      onOpenFile?.(file.path, "file");
    }
  };

  const handleNewWorkbook = () => {
    setCreatingWorkbook(true);
    setWorkbookName("");
  };

  const handleCreateWorkbook = async (e) => {
    e.preventDefault();
    if (!workbookName.trim()) return;

    try {
      const workbooksDir = `${projectRoot}/notebooks`;
      const workbookPath = await invoke("create_workbook", {
        workbookPath: workbooksDir,
        workbookName: workbookName,
      });

      setCreatingWorkbook(false);
      setWorkbookName("");
      await loadProjectFiles();
      onOpenFile?.(workbookPath, "workbook");
      updateRecentWorkbook(workbookPath);
    } catch (err) {
      console.error("Failed to create workbook:", err);
    }
  };

  const handleFileAction = async (action, file) => {
    if (action === 'rename') {
      setRenamingFile(file);
    } else if (action === 'delete') {
      const confirmed = await ask(
        `Are you sure you want to delete "${file.name}"? This cannot be undone.`,
        {
          title: "Delete File",
          kind: "warning",
          okLabel: "Delete",
          cancelLabel: "Cancel",
        }
      );

      if (confirmed) {
        try {
          await invoke("delete_file", { filePath: file.path });
          onFileDeleted?.(file.path);
          await loadProjectFiles();
        } catch (err) {
          console.error("Failed to delete:", err);
        }
      }
    }
  };

  const handleRenameConfirm = async (newName) => {
    if (!renamingFile) return;

    try {
      await invoke("rename_file", {
        oldPath: renamingFile.path,
        newName: newName,
      });
      setRenamingFile(null);
      await loadProjectFiles();
    } catch (err) {
      console.error("Failed to rename:", err);
    }
  };

  // Get workbooks ordered by recent use
  const orderedWorkbooks = [...workbooks].sort((a, b) => {
    const aIndex = recentWorkbooks.indexOf(a.path);
    const bIndex = recentWorkbooks.indexOf(b.path);

    if (aIndex === -1 && bIndex === -1) return 0;
    if (aIndex === -1) return 1;
    if (bIndex === -1) return -1;
    return aIndex - bIndex;
  });

  return (
    <div className="flex flex-col h-full select-none bg-gray-50">
      {/* Project Name Header */}
      <div className="px-4 py-4 border-b border-gray-200 bg-white">
        <h3 className="text-sm font-semibold text-gray-900">{projectName}</h3>
      </div>

      <div className="flex-1 overflow-y-auto">
        {/* Workbooks Section */}
        <SidebarSection
          title="Workbooks"
          icon="📓"
          defaultExpanded={true}
          onHeaderClick={() => setShowWorkbooksTable(true)}
        >
          <div className="px-2">
            <button
              onClick={handleNewWorkbook}
              className="w-full px-3 py-2 mb-2 text-xs font-medium text-blue-600 bg-blue-50 hover:bg-blue-100 rounded transition-colors flex items-center justify-center gap-2"
            >
              <span>+</span>
              <span>New Workbook</span>
            </button>

            {creatingWorkbook && (
              <div className="mb-2 p-2 bg-white border border-gray-200 rounded">
                <form onSubmit={handleCreateWorkbook} className="flex flex-col gap-2">
                  <input
                    type="text"
                    value={workbookName}
                    onChange={(e) => setWorkbookName(e.target.value)}
                    placeholder="Workbook name"
                    className="w-full px-2 py-1.5 text-sm border border-gray-300 rounded focus:outline-none focus:ring-2 focus:ring-blue-500"
                    autoFocus
                  />
                  <div className="flex gap-1.5">
                    <button
                      type="submit"
                      className="flex-1 px-3 py-1.5 text-xs font-medium text-white bg-blue-600 hover:bg-blue-700 rounded"
                    >
                      Create
                    </button>
                    <button
                      type="button"
                      onClick={() => setCreatingWorkbook(false)}
                      className="flex-1 px-3 py-1.5 text-xs font-medium text-gray-700 bg-white hover:bg-gray-50 border border-gray-300 rounded"
                    >
                      Cancel
                    </button>
                  </div>
                </form>
              </div>
            )}

            {orderedWorkbooks.length === 0 && !loading && (
              <p className="text-xs text-gray-400 text-center py-4">No workbooks yet</p>
            )}

            {orderedWorkbooks.map((workbook) => {
              const isActive = activeFilePath === workbook.path;
              return (
                <div
                  key={workbook.path}
                  onClick={() => handleFileClick(workbook)}
                  className={`px-3 py-2 mb-1 text-sm rounded cursor-pointer transition-all ${
                    isActive
                      ? 'bg-blue-50 text-blue-900 font-medium border-l-2 border-blue-500'
                      : 'text-gray-700 hover:bg-white'
                  }`}
                >
                  <div className="flex items-center gap-2">
                    <span className="text-xs">📓</span>
                    <span className="flex-1 overflow-hidden text-ellipsis whitespace-nowrap text-xs">
                      {workbook.name.replace('.ipynb', '')}
                    </span>
                  </div>
                </div>
              );
            })}
          </div>
        </SidebarSection>

        {/* Secrets Section (Placeholder) */}
        <SidebarSection title="Secrets" icon="🔐" defaultExpanded={false}>
          <div className="px-4 py-6 text-center">
            <p className="text-xs text-gray-400 mb-3">
              Securely store API keys and credentials
            </p>
            <button className="px-4 py-2 text-xs font-medium text-gray-600 bg-gray-100 hover:bg-gray-200 rounded transition-colors">
              Add Secret
            </button>
          </div>
        </SidebarSection>

        {/* Schedule Section (Placeholder) */}
        <SidebarSection title="Schedule" icon="⏰" defaultExpanded={false}>
          <div className="px-4">
            <div className="flex border-b border-gray-200 mb-2">
              <button className="flex-1 px-3 py-2 text-xs font-medium text-blue-600 border-b-2 border-blue-600">
                Scheduled
              </button>
              <button className="flex-1 px-3 py-2 text-xs font-medium text-gray-500 hover:text-gray-700">
                Recent Runs
              </button>
            </div>
            <div className="py-6 text-center">
              <p className="text-xs text-gray-400 mb-3">
                No scheduled workbooks
              </p>
              <button className="px-4 py-2 text-xs font-medium text-gray-600 bg-gray-100 hover:bg-gray-200 rounded transition-colors">
                Add Schedule
              </button>
            </div>
          </div>
        </SidebarSection>

        {/* Files Section */}
        <SidebarSection title="Files" icon="📁" defaultExpanded={true}>
          <div className="px-1">
            {loading && <div className="px-4 py-3 text-xs text-gray-500">Loading...</div>}

            {!loading && files.length === 0 && (
              <div className="px-4 py-8 text-center text-xs text-gray-400">
                No files yet
              </div>
            )}

            {!loading && files.map((file) => (
              <FileTreeItem
                key={file.path}
                file={file}
                level={0}
                onFileClick={handleFileClick}
                onFileAction={handleFileAction}
                activeFilePath={activeFilePath}
              />
            ))}
          </div>
        </SidebarSection>
      </div>

      {/* Project Settings at Bottom */}
      <div className="border-t border-gray-200 bg-white">
        <button
          className="w-full px-4 py-3 flex items-center gap-2 text-xs font-medium text-gray-600 hover:bg-gray-50 transition-colors"
          onClick={() => alert('Project Settings - Coming Soon')}
        >
          <span>⚙️</span>
          <span>Project Settings</span>
        </button>
      </div>

      {/* Rename Dialog */}
      {renamingFile && (
        <InputDialog
          title="Rename File"
          label="New name:"
          initialValue={renamingFile.name}
          placeholder="Enter new name"
          onConfirm={handleRenameConfirm}
          onCancel={() => setRenamingFile(null)}
        />
      )}

      {/* Workbooks Table View Modal */}
      {showWorkbooksTable && (
        <WorkbooksTableView
          workbooks={orderedWorkbooks}
          onClose={() => setShowWorkbooksTable(false)}
          onOpenWorkbook={handleFileClick}
        />
      )}
    </div>
  );
}
