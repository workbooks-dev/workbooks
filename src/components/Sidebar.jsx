import { useState, useEffect, useRef } from "react";
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
function FileTreeItem({ file, level = 0, onFileClick, onFileAction, activeFilePath, projectRoot, showPath = false, isRenaming, onRenameComplete, onRenameCancel, renamingFilePath }) {
  const [isExpanded, setIsExpanded] = useState(false);
  const [children, setChildren] = useState([]);
  const [loading, setLoading] = useState(false);
  const [showContextMenu, setShowContextMenu] = useState(false);
  const [contextMenuPos, setContextMenuPos] = useState({ x: 0, y: 0 });
  const [renamingValue, setRenamingValue] = useState(file.name);
  const renameInputRef = useRef(null);

  // Focus and select text when renaming starts
  useEffect(() => {
    if (isRenaming && renameInputRef.current) {
      renameInputRef.current.focus();
      // Select filename without extension
      const nameWithoutExt = file.name.replace(/\.[^/.]+$/, "");
      renameInputRef.current.setSelectionRange(0, nameWithoutExt.length);
    }
  }, [isRenaming]);

  // Get relative path for display in search results
  const getRelativePath = () => {
    if (!showPath || !projectRoot) return null;
    const relativePath = file.path.replace(projectRoot, '').replace(/^\//, '');
    const pathParts = relativePath.split('/');
    if (pathParts.length > 1) {
      return pathParts.slice(0, -1).join('/');
    }
    return null;
  };

  const handleRenameKeyDown = (e) => {
    if (e.key === 'Enter') {
      e.preventDefault();
      if (renamingValue.trim() && renamingValue !== file.name) {
        onRenameComplete?.(file, renamingValue.trim());
      } else {
        onRenameCancel?.();
      }
    } else if (e.key === 'Escape') {
      e.preventDefault();
      onRenameCancel?.();
    }
  };

  const handleRenameBlur = () => {
    // When clicking away, treat as cancel
    onRenameCancel?.();
  };

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

  const relativePath = getRelativePath();

  return (
    <>
      <div
        className={`flex items-center gap-2 px-3 py-1.5 text-sm rounded mx-2 my-0.5 transition-all ${
          isActive
            ? 'bg-blue-50 text-blue-900 font-medium border-l-2 border-blue-500'
            : 'text-gray-700 hover:bg-white'
        } ${!isRenaming ? 'cursor-pointer' : ''}`}
        style={{ paddingLeft: `${level * 12 + 12}px` }}
        onClick={!isRenaming ? handleToggle : undefined}
        onContextMenu={!isRenaming ? handleContextMenu : undefined}
      >
        <span className="text-xs opacity-60 w-4 text-center flex-shrink-0">{getFileIcon()}</span>
        <div className="flex-1 overflow-hidden">
          {isRenaming ? (
            <input
              ref={renameInputRef}
              type="text"
              value={renamingValue}
              onChange={(e) => setRenamingValue(e.target.value)}
              onKeyDown={handleRenameKeyDown}
              onBlur={handleRenameBlur}
              className="w-full px-1 py-0.5 text-xs border border-blue-500 rounded focus:outline-none focus:ring-1 focus:ring-blue-500 bg-white"
              onClick={(e) => e.stopPropagation()}
            />
          ) : (
            <>
              <span className="overflow-hidden text-ellipsis whitespace-nowrap block">{file.name}</span>
              {relativePath && (
                <span className="text-xs text-gray-500 overflow-hidden text-ellipsis whitespace-nowrap block">
                  {relativePath}
                </span>
              )}
            </>
          )}
        </div>
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
              isRenaming={child.path === renamingFilePath}
              onRenameComplete={onRenameComplete}
              onRenameCancel={onRenameCancel}
              renamingFilePath={renamingFilePath}
            />
          ))}
        </div>
      )}
    </>
  );
}

export function Sidebar({ projectRoot, projectName, onOpenFile, onFileDeleted, onFileRenamed, activeFilePath, onOpenSettings }) {
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
  const [searchResults, setSearchResults] = useState([]);
  const [searching, setSearching] = useState(false);
  const [creatingFile, setCreatingFile] = useState(false);
  const [creatingFolder, setCreatingFolder] = useState(false);
  const [newItemName, setNewItemName] = useState("");
  const [fileInfo, setFileInfo] = useState(null);
  const [creatingInFolder, setCreatingInFolder] = useState(null);
  const [renamingFilePath, setRenamingFilePath] = useState(null);

  // Refs for file/folder creation inputs to ensure focus
  const fileInputRef = useRef(null);
  const folderInputRef = useRef(null);

  // Focus file input when creatingFile becomes true
  useEffect(() => {
    if (creatingFile && fileInputRef.current) {
      // Use setTimeout to ensure DOM is ready
      setTimeout(() => {
        fileInputRef.current?.focus();
      }, 0);
    }
  }, [creatingFile]);

  // Focus folder input when creatingFolder becomes true
  useEffect(() => {
    if (creatingFolder && folderInputRef.current) {
      // Use setTimeout to ensure DOM is ready
      setTimeout(() => {
        folderInputRef.current?.focus();
      }, 0);
    }
  }, [creatingFolder]);

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

    // Listen for file system changes from file watcher
    let unlistenFileSystem;
    const setupFileWatcher = async () => {
      const { listen } = await import("@tauri-apps/api/event");
      unlistenFileSystem = await listen("file-system-changed", () => {
        console.log("File system changed, refreshing file list");
        loadProjectFiles();
      });
    };

    setupFileWatcher();

    window.addEventListener("workbooks:files-changed", handleFilesChanged);
    window.addEventListener("workbooks:secrets-changed", handleSecretsChanged);

    return () => {
      window.removeEventListener("workbooks:files-changed", handleFilesChanged);
      window.removeEventListener("workbooks:secrets-changed", handleSecretsChanged);
      if (unlistenFileSystem) {
        unlistenFileSystem();
      }
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
    const projectKey = `workbooks_recent_workbooks_${projectRoot}`;
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
    const projectKey = `workbooks_recent_workbooks_${projectRoot}`;
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

  // Recursively collect all files from a folder tree
  const getAllFilesRecursive = async (dirPath) => {
    try {
      const fileList = await invoke("list_files", {
        directoryPath: dirPath,
      });

      const allFiles = [];
      for (const file of fileList) {
        allFiles.push(file);
        if (file.is_dir) {
          const subFiles = await getAllFilesRecursive(file.path);
          allFiles.push(...subFiles);
        }
      }
      return allFiles;
    } catch (err) {
      console.error("Failed to load folder contents:", err);
      return [];
    }
  };

  // Handle file search - debounce and search recursively
  useEffect(() => {
    const performSearch = async () => {
      if (!fileSearchQuery.trim()) {
        setSearching(false);
        setSearchResults([]);
        return;
      }

      setSearching(true);
      try {
        // Get all files recursively
        const allFiles = await getAllFilesRecursive(projectRoot);
        // Filter files (not folders)
        const filesOnly = allFiles.filter(f => !f.is_dir && f.extension !== "ipynb");
        // Filter by search query
        const results = filterFiles(filesOnly, fileSearchQuery);
        setSearchResults(results);
      } catch (err) {
        console.error("Search failed:", err);
        setSearchResults([]);
      } finally {
        setSearching(false);
      }
    };

    // Debounce search
    const timeoutId = setTimeout(performSearch, 300);
    return () => clearTimeout(timeoutId);
  }, [fileSearchQuery, projectRoot]);

  const filteredFiles = filterFiles(files, fileSearchQuery);
  const displayFiles = fileSearchQuery.trim() ? searchResults : files;

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
        // Start inline renaming
        setRenamingFilePath(file.path);
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
      const newPath = await invoke("rename_file", {
        oldPath: renamingFile.path,
        newName: newName,
      });

      // Notify parent about the rename so tabs can be updated
      if (onFileRenamed) {
        onFileRenamed(renamingFile.path, newPath);
      }

      setRenamingFile(null);
      await loadProjectFiles();
    } catch (err) {
      console.error("Failed to rename:", err);
    }
  };

  const handleInlineRenameComplete = async (file, newName) => {
    try {
      const newPath = await invoke("rename_file", {
        oldPath: file.path,
        newName: newName,
      });

      // Notify parent about the rename so tabs can be updated
      if (onFileRenamed) {
        onFileRenamed(file.path, newPath);
      }

      setRenamingFilePath(null);
      await loadProjectFiles();
    } catch (err) {
      console.error("Failed to rename:", err);
      alert(`Failed to rename: ${err}`);
      setRenamingFilePath(null);
    }
  };

  const handleInlineRenameCancel = () => {
    setRenamingFilePath(null);
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

        {/* Schedule Section */}
        <SidebarSection
          title="Schedule"
          defaultExpanded={false}
          onHeaderClick={() => onOpenFile?.('__schedule__', 'schedule')}
        >
          <div className="px-4 py-4">
            <div className="text-center mb-3">
              <p className="text-sm font-medium text-gray-700">
                Scheduled workbooks
              </p>
              <p className="text-xs text-gray-400 mt-1">
                Automate your pipelines
              </p>
            </div>
            <button
              className="w-full px-4 py-2 text-xs font-medium text-blue-600 bg-blue-50 hover:bg-blue-100 rounded transition-colors"
              onClick={() => onOpenFile?.('__schedule__', 'schedule')}
            >
              Manage Schedule
            </button>
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
                  ref={fileInputRef}
                  type="text"
                  value={newItemName}
                  onChange={(e) => setNewItemName(e.target.value)}
                  placeholder="File name (e.g., data.csv)"
                  className="w-full px-2 py-1.5 text-sm border border-gray-300 rounded focus:outline-none focus:ring-2 focus:ring-blue-500 mb-2"
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
                  ref={folderInputRef}
                  type="text"
                  value={newItemName}
                  onChange={(e) => setNewItemName(e.target.value)}
                  placeholder="Folder name"
                  className="w-full px-2 py-1.5 text-sm border border-gray-300 rounded focus:outline-none focus:ring-2 focus:ring-blue-500 mb-2"
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
            {searching && <div className="px-4 py-3 text-xs text-gray-500">Searching...</div>}

            {!loading && files.length === 0 && (
              <div className="px-4 py-8 text-center text-xs text-gray-400">
                No files yet
              </div>
            )}

            {!loading && !searching && fileSearchQuery.trim() && searchResults.length === 0 && (
              <div className="px-4 py-8 text-center text-xs text-gray-400">
                No files match "{fileSearchQuery}"
              </div>
            )}

            {!loading && !searching && fileSearchQuery.trim() && searchResults.length > 0 && (
              <>
                <div className="px-4 py-2 text-xs text-gray-500">
                  Found {searchResults.length} file{searchResults.length !== 1 ? 's' : ''}
                </div>
                {searchResults.map((file) => (
                  <FileTreeItem
                    key={file.path}
                    file={file}
                    level={0}
                    onFileClick={handleFileClick}
                    onFileAction={handleFileAction}
                    activeFilePath={activeFilePath}
                    projectRoot={projectRoot}
                    showPath={true}
                    isRenaming={file.path === renamingFilePath}
                    onRenameComplete={handleInlineRenameComplete}
                    onRenameCancel={handleInlineRenameCancel}
                    renamingFilePath={renamingFilePath}
                  />
                ))}
              </>
            )}

            {!loading && !fileSearchQuery.trim() && files.map((file) => (
              <FileTreeItem
                key={file.path}
                file={file}
                level={0}
                onFileClick={handleFileClick}
                onFileAction={handleFileAction}
                activeFilePath={activeFilePath}
                projectRoot={projectRoot}
                isRenaming={file.path === renamingFilePath}
                onRenameComplete={handleInlineRenameComplete}
                onRenameCancel={handleInlineRenameCancel}
                renamingFilePath={renamingFilePath}
              />
            ))}
          </div>
        </SidebarSection>
      </div>

      {/* App Settings at Bottom */}
      <div className="border-t border-gray-200 bg-white">
        <button
          className="w-full px-4 py-3 text-xs font-medium text-gray-600 hover:bg-gray-50 transition-colors text-left flex items-center gap-2"
          onClick={() => onOpenSettings && onOpenSettings()}
        >
          <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M10.325 4.317c.426-1.756 2.924-1.756 3.35 0a1.724 1.724 0 002.573 1.066c1.543-.94 3.31.826 2.37 2.37a1.724 1.724 0 001.065 2.572c1.756.426 1.756 2.924 0 3.35a1.724 1.724 0 00-1.066 2.573c.94 1.543-.826 3.31-2.37 2.37a1.724 1.724 0 00-2.572 1.065c-.426 1.756-2.924 1.756-3.35 0a1.724 1.724 0 00-2.573-1.066c-1.543.94-3.31-.826-2.37-2.37a1.724 1.724 0 00-1.065-2.572c-1.756-.426-1.756-2.924 0-3.35a1.724 1.724 0 001.066-2.573c-.94-1.543.826-3.31 2.37-2.37.996.608 2.296.07 2.572-1.065z" />
            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M15 12a3 3 0 11-6 0 3 3 0 016 0z" />
          </svg>
          Settings
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
