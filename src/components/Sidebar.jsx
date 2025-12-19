import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { ask } from "@tauri-apps/plugin-dialog";
import { writeText } from "@tauri-apps/plugin-clipboard-manager";
import { ContextMenu } from "./ContextMenu";
import { InputDialog } from "./InputDialog";
import { WorkbooksTableView } from "./WorkbooksTableView";
import { FileInfoDialog } from "./FileInfoDialog";

// Collapsible section component
function SidebarSection({ title, children, defaultExpanded = true, onHeaderClick }) {
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
        <h3 className="text-xs font-semibold uppercase tracking-wider text-gray-700">
          {title}
        </h3>
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
function FileTreeItem({ file, level = 0, onFileClick, onFileAction, activeFilePath, projectRoot }) {
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
          // Include all files, including .ipynb files in folders
          setChildren(fileList);
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
    // No icons for files, just show the filename
    return "";
  };

  const getContextMenuItems = () => {
    const items = [];

    // Folder-specific options
    if (file.is_dir) {
      items.push(
        { label: "New File", action: () => onFileAction('newFile', file) },
        { label: "New Folder", action: () => onFileAction('newFolder', file) },
        { type: 'separator' }
      );
    }

    // Common options
    items.push(
      { label: "Rename", action: () => onFileAction('rename', file) },
      { label: "Delete", action: () => onFileAction('delete', file) },
      { type: 'separator' },
      { label: "Reveal in Finder", action: () => onFileAction('revealInFinder', file) },
      { label: "Copy Path", action: () => onFileAction('copyPath', file) },
      { label: "Copy Relative Path", action: () => onFileAction('copyRelativePath', file, projectRoot) },
      { type: 'separator' },
      { label: "Get Info", action: () => onFileAction('getInfo', file) }
    );

    return items;
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
              projectRoot={projectRoot}
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
  const [secretsCount, setSecretsCount] = useState(0);
  const [fileSearchQuery, setFileSearchQuery] = useState("");
  const [creatingFile, setCreatingFile] = useState(false);
  const [creatingFolder, setCreatingFolder] = useState(false);
  const [newItemName, setNewItemName] = useState("");
  const [fileInfo, setFileInfo] = useState(null);
  const [creatingInFolder, setCreatingInFolder] = useState(null);

  useEffect(() => {
    loadProjectFiles();
    loadRecentWorkbooks();
    loadSecretsCount();

    // Listen for file changes (e.g., when files are dropped)
    const handleFilesChanged = () => {
      loadProjectFiles();
    };

    // Listen for secrets changes
    const handleSecretsChanged = () => {
      loadSecretsCount();
    };

    window.addEventListener("tether:files-changed", handleFilesChanged);
    window.addEventListener("tether:secrets-changed", handleSecretsChanged);

    return () => {
      window.removeEventListener("tether:files-changed", handleFilesChanged);
      window.removeEventListener("tether:secrets-changed", handleSecretsChanged);
    };
  }, [projectRoot]);

  const loadProjectFiles = async () => {
    setLoading(true);
    try {
      const fileList = await invoke("list_files", {
        directoryPath: projectRoot,
      });

      // Separate workbooks from other files
      const notebookFiles = fileList.filter(f => f.extension === "ipynb" && f.name !== "notebooks");
      // Include notebooks folder in FILES section, but exclude root-level .ipynb files
      const otherFiles = fileList.filter(f => f.extension !== "ipynb");

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

  const loadSecretsCount = async () => {
    try {
      const secrets = await invoke("list_secrets", {
        projectPath: projectRoot,
      });
      setSecretsCount(secrets.length);
    } catch (err) {
      console.error("Failed to load secrets count:", err);
      setSecretsCount(0);
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

  const filterFiles = (fileList, query) => {
    if (!query.trim()) return fileList;

    const lowerQuery = query.toLowerCase();
    return fileList.filter(file =>
      file.name.toLowerCase().includes(lowerQuery)
    );
  };

  const filteredFiles = filterFiles(files, fileSearchQuery);

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

  const handleFileAction = async (action, file, extraData) => {
    switch (action) {
      case 'rename':
        setRenamingFile(file);
        break;

      case 'delete':
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
        break;

      case 'newFile':
        setCreatingFile(true);
        setCreatingInFolder(file);
        setNewItemName("");
        break;

      case 'newFolder':
        setCreatingFolder(true);
        setCreatingInFolder(file);
        setNewItemName("");
        break;

      case 'revealInFinder':
        try {
          await invoke("reveal_in_finder", { filePath: file.path });
        } catch (err) {
          console.error("Failed to reveal in finder:", err);
          alert(`Failed to reveal in finder: ${err}`);
        }
        break;

      case 'copyPath':
        try {
          await writeText(file.path);
        } catch (err) {
          console.error("Failed to copy path:", err);
        }
        break;

      case 'copyRelativePath':
        try {
          const relativePath = file.path.replace(projectRoot + '/', '');
          await writeText(relativePath);
        } catch (err) {
          console.error("Failed to copy relative path:", err);
        }
        break;

      case 'getInfo':
        try {
          const info = await invoke("get_file_info", { filePath: file.path });
          setFileInfo(info);
        } catch (err) {
          console.error("Failed to get file info:", err);
          alert(`Failed to get file info: ${err}`);
        }
        break;

      default:
        break;
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

  const handleCreateFile = async (e) => {
    e.preventDefault();
    if (!newItemName.trim()) return;

    try {
      const parentPath = creatingInFolder ? creatingInFolder.path : projectRoot;
      await invoke("create_new_file", {
        parentPath: parentPath,
        fileName: newItemName,
        initialContent: "",
      });
      setCreatingFile(false);
      setNewItemName("");
      setCreatingInFolder(null);
      await loadProjectFiles();
    } catch (err) {
      console.error("Failed to create file:", err);
      alert(`Failed to create file: ${err}`);
    }
  };

  const handleCreateFolder = async (e) => {
    e.preventDefault();
    if (!newItemName.trim()) return;

    try {
      const parentPath = creatingInFolder ? creatingInFolder.path : projectRoot;
      await invoke("create_new_folder", {
        parentPath: parentPath,
        folderName: newItemName,
      });
      setCreatingFolder(false);
      setNewItemName("");
      setCreatingInFolder(null);
      await loadProjectFiles();
    } catch (err) {
      console.error("Failed to create folder:", err);
      alert(`Failed to create folder: ${err}`);
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
                  <span className="flex-1 overflow-hidden text-ellipsis whitespace-nowrap text-xs">
                    {workbook.name.replace('.ipynb', '')}
                  </span>
                </div>
              );
            })}
          </div>
        </SidebarSection>

        {/* Secrets Section */}
        <SidebarSection
          title="Secrets"
          defaultExpanded={false}
          onHeaderClick={() => onOpenFile?.('__secrets__', 'secrets')}
        >
          <div className="px-4 py-4">
            <div className="text-center mb-3">
              <p className="text-sm font-medium text-gray-700">
                {secretsCount === 0 ? 'No secrets' : `${secretsCount} secret${secretsCount === 1 ? '' : 's'}`}
              </p>
              <p className="text-xs text-gray-400 mt-1">
                Securely stored and encrypted
              </p>
            </div>
            <button
              className="w-full px-4 py-2 text-xs font-medium text-blue-600 bg-blue-50 hover:bg-blue-100 rounded transition-colors"
              onClick={() => onOpenFile?.('__secrets__', 'secrets')}
            >
              Manage Secrets
            </button>
          </div>
        </SidebarSection>

        {/* Schedule Section (Placeholder) */}
        <SidebarSection title="Schedule" defaultExpanded={false}>
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
        <SidebarSection title="Files" defaultExpanded={true}>
          <div className="px-2 pb-2">
            <input
              type="text"
              placeholder="Search files..."
              value={fileSearchQuery}
              onChange={(e) => setFileSearchQuery(e.target.value)}
              className="w-full px-3 py-1.5 text-xs border border-gray-300 rounded focus:outline-none focus:ring-2 focus:ring-blue-500 bg-white"
            />
            <div className="flex gap-2 mt-2">
              <button
                onClick={() => { setCreatingFile(true); setNewItemName(""); }}
                className="flex-1 px-3 py-1.5 text-xs font-medium text-gray-700 bg-white hover:bg-gray-50 border border-gray-300 rounded transition-colors"
              >
                + File
              </button>
              <button
                onClick={() => { setCreatingFolder(true); setNewItemName(""); }}
                className="flex-1 px-3 py-1.5 text-xs font-medium text-gray-700 bg-white hover:bg-gray-50 border border-gray-300 rounded transition-colors"
              >
                + Folder
              </button>
            </div>
          </div>

          {creatingFile && (
            <div className="px-2 pb-2">
              <form onSubmit={handleCreateFile} className="p-2 bg-white border border-gray-200 rounded">
                <input
                  type="text"
                  value={newItemName}
                  onChange={(e) => setNewItemName(e.target.value)}
                  placeholder="File name (e.g., data.csv)"
                  className="w-full px-2 py-1.5 text-sm border border-gray-300 rounded focus:outline-none focus:ring-2 focus:ring-blue-500 mb-2"
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
                    onClick={() => { setCreatingFile(false); setNewItemName(""); setCreatingInFolder(null); }}
                    className="flex-1 px-3 py-1.5 text-xs font-medium text-gray-700 bg-white hover:bg-gray-50 border border-gray-300 rounded"
                  >
                    Cancel
                  </button>
                </div>
              </form>
            </div>
          )}

          {creatingFolder && (
            <div className="px-2 pb-2">
              <form onSubmit={handleCreateFolder} className="p-2 bg-white border border-gray-200 rounded">
                <input
                  type="text"
                  value={newItemName}
                  onChange={(e) => setNewItemName(e.target.value)}
                  placeholder="Folder name"
                  className="w-full px-2 py-1.5 text-sm border border-gray-300 rounded focus:outline-none focus:ring-2 focus:ring-blue-500 mb-2"
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
                    onClick={() => { setCreatingFolder(false); setNewItemName(""); setCreatingInFolder(null); }}
                    className="flex-1 px-3 py-1.5 text-xs font-medium text-gray-700 bg-white hover:bg-gray-50 border border-gray-300 rounded"
                  >
                    Cancel
                  </button>
                </div>
              </form>
            </div>
          )}

          <div className="px-1">
            {loading && <div className="px-4 py-3 text-xs text-gray-500">Loading...</div>}

            {!loading && files.length === 0 && (
              <div className="px-4 py-8 text-center text-xs text-gray-400">
                No files yet
              </div>
            )}

            {!loading && filteredFiles.length === 0 && files.length > 0 && (
              <div className="px-4 py-8 text-center text-xs text-gray-400">
                No files match "{fileSearchQuery}"
              </div>
            )}

            {!loading && filteredFiles.map((file) => (
              <FileTreeItem
                key={file.path}
                file={file}
                level={0}
                onFileClick={handleFileClick}
                onFileAction={handleFileAction}
                activeFilePath={activeFilePath}
                projectRoot={projectRoot}
              />
            ))}
          </div>
        </SidebarSection>
      </div>

      {/* Project Settings at Bottom */}
      <div className="border-t border-gray-200 bg-white">
        <button
          className="w-full px-4 py-3 text-xs font-medium text-gray-600 hover:bg-gray-50 transition-colors text-left"
          onClick={() => alert('Project Settings - Coming Soon')}
        >
          Project Settings
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

      {/* File Info Dialog */}
      {fileInfo && (
        <FileInfoDialog
          fileInfo={fileInfo}
          onClose={() => setFileInfo(null)}
        />
      )}
    </div>
  );
}
