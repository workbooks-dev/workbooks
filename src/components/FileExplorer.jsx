import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { ask } from "@tauri-apps/plugin-dialog";
import { ContextMenu } from "./ContextMenu";
import { InputDialog } from "./InputDialog";

function FileTreeItem({ file, level = 0, onFileClick, onFileAction, activeFilePath }) {
  const [isExpanded, setIsExpanded] = useState(false);
  const [children, setChildren] = useState([]);
  const [loading, setLoading] = useState(false);
  const [showContextMenu, setShowContextMenu] = useState(false);
  const [contextMenuPos, setContextMenuPos] = useState({ x: 0, y: 0 });

  const handleToggle = async () => {
    if (file.is_dir) {
      if (!isExpanded) {
        // Load children
        setLoading(true);
        try {
          const fileList = await invoke("list_files", {
            directoryPath: file.path,
          });
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

    switch (file.extension) {
      case "ipynb":
        return "📓";
      case "py":
        return "🐍";
      case "md":
        return "📝";
      case "json":
      case "toml":
      case "yaml":
      case "yml":
        return "⚙️";
      default:
        return "📄";
    }
  };

  const getContextMenuItems = () => {
    const items = [];

    if (file.extension === "ipynb") {
      items.push({ label: "Duplicate", action: () => onFileAction('duplicate', file) });
    }

    items.push(
      { label: "Rename", action: () => onFileAction('rename', file) },
      { label: "Delete", action: () => onFileAction('delete', file) }
    );

    return items;
  };

  const isActive = activeFilePath === file.path;

  return (
    <>
      <div
        className={`flex items-center gap-2 px-3 py-1.5 cursor-pointer text-sm rounded mx-2 my-0.5 transition-all ${
          isActive
            ? 'bg-blue-50 text-blue-900 font-medium shadow-soft border-l-2 border-blue-500'
            : 'text-gray-900 hover:bg-white hover:shadow-soft'
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
        <div className="tree-children">
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

export function FileExplorer({ projectRoot, projectName, onOpenWorkbook, onFileDeleted, activeFilePath }) {
  const [files, setFiles] = useState([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState(null);
  const [creatingWorkbook, setCreatingWorkbook] = useState(false);
  const [workbookName, setWorkbookName] = useState("");
  const [renamingFile, setRenamingFile] = useState(null);
  const [duplicatingFile, setDuplicatingFile] = useState(null);

  useEffect(() => {
    loadRootFiles();
  }, [projectRoot]);

  const loadRootFiles = async () => {
    setLoading(true);
    setError(null);

    try {
      const fileList = await invoke("list_files", {
        directoryPath: projectRoot,
      });
      setFiles(fileList);
    } catch (err) {
      console.error("Failed to load files:", err);
      setError(err.toString());
    } finally {
      setLoading(false);
    }
  };

  const handleFileClick = (file) => {
    if (file.extension === "ipynb") {
      onOpenWorkbook?.(file.path, "workbook");
    } else {
      onOpenWorkbook?.(file.path, "file");
    }
  };

  const handleNewWorkbook = () => {
    setCreatingWorkbook(true);
    setWorkbookName("");
  };

  const handleCreateWorkbook = async (e) => {
    e.preventDefault();

    if (!workbookName.trim()) {
      return;
    }

    try {
      const workbooksDir = `${projectRoot}/notebooks`;
      const workbookPath = await invoke("create_workbook", {
        workbookPath: workbooksDir,
        workbookName: workbookName,
      });

      console.log("Created workbook:", workbookPath);
      setCreatingWorkbook(false);
      setWorkbookName("");

      // Refresh the file list
      await loadRootFiles();

      // Automatically open the newly created workbook
      onOpenWorkbook?.(workbookPath, "workbook");
    } catch (err) {
      console.error("Failed to create workbook:", err);
      setError(err.toString());
    }
  };

  const handleCancelCreate = () => {
    setCreatingWorkbook(false);
    setWorkbookName("");
  };

  const handleFileAction = (action, file) => {
    if (action === 'rename') {
      setRenamingFile(file);
    } else if (action === 'delete') {
      handleDeleteFile(file);
    } else if (action === 'duplicate') {
      setDuplicatingFile(file);
    }
  };

  const handleRenameConfirm = async (newName) => {
    if (!renamingFile) return;

    try {
      const newPath = await invoke("rename_file", {
        oldPath: renamingFile.path,
        newName: newName,
      });

      console.log("Renamed to:", newPath);
      setRenamingFile(null);

      // Refresh the file list
      await loadRootFiles();
    } catch (err) {
      console.error("Failed to rename:", err);
      setError(err.toString());
    }
  };

  const handleDeleteFile = async (file) => {
    const confirmed = await ask(
      `Are you sure you want to delete "${file.name}"? This cannot be undone.`,
      {
        title: "Delete File",
        kind: "warning",
        okLabel: "Delete",
        cancelLabel: "Cancel",
      }
    );

    if (!confirmed) return;

    try {
      await invoke("delete_file", { filePath: file.path });

      console.log("Deleted:", file.path);

      // Notify parent if this is an open file
      if (onFileDeleted) {
        onFileDeleted(file.path);
      }

      // Refresh the file list
      await loadRootFiles();
    } catch (err) {
      console.error("Failed to delete:", err);
      setError(err.toString());
    }
  };

  const handleDuplicateConfirm = async (newName) => {
    if (!duplicatingFile) return;

    try {
      const newPath = await invoke("duplicate_workbook", {
        sourcePath: duplicatingFile.path,
        newName: newName,
      });

      console.log("Duplicated to:", newPath);
      setDuplicatingFile(null);

      // Refresh the file list
      await loadRootFiles();

      // Optionally, open the new workbook
      // onOpenWorkbook(newPath, "workbook");
    } catch (err) {
      console.error("Failed to duplicate:", err);
      setError(err.toString());
    }
  };

  return (
    <div className="flex flex-col h-full select-none">
      <div className="px-4 py-4 border-b border-gray-200 flex items-center justify-between">
        <h3 className="text-xs font-semibold uppercase tracking-wider text-gray-500">{projectName}</h3>
        <button
          className="w-6 h-6 p-0 text-lg leading-none rounded bg-blue-600 text-white hover:bg-blue-700 transition-colors flex items-center justify-center shadow-sm"
          onClick={handleNewWorkbook}
          title="Create new workbook"
        >
          +
        </button>
      </div>

      {creatingWorkbook && (
        <div className="px-4 py-3 border-b border-gray-200 bg-gray-50">
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
                className="flex-1 px-3 py-1.5 text-xs font-medium text-white bg-blue-600 hover:bg-blue-700 rounded transition-colors"
              >
                Create
              </button>
              <button
                type="button"
                onClick={handleCancelCreate}
                className="flex-1 px-3 py-1.5 text-xs font-medium text-gray-700 bg-white hover:bg-gray-50 border border-gray-300 rounded transition-colors"
              >
                Cancel
              </button>
            </div>
          </form>
        </div>
      )}

      {loading && <div className="px-4 py-3 text-xs text-gray-500">Loading...</div>}

      {error && (
        <div className="px-4 py-3 text-xs text-red-600">
          {error}
        </div>
      )}

      {!loading && !error && (
        <div className="flex-1 overflow-y-auto py-1 custom-scrollbar">
          {files.map((file) => (
            <FileTreeItem
              key={file.path}
              file={file}
              level={0}
              onFileClick={handleFileClick}
              onFileAction={handleFileAction}
              activeFilePath={activeFilePath}
            />
          ))}
          {files.length === 0 && (
            <div className="px-4 py-8 text-center text-xs text-gray-400">No files</div>
          )}
        </div>
      )}

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

      {duplicatingFile && (
        <InputDialog
          title="Duplicate Notebook"
          label="New name:"
          initialValue={duplicatingFile.name.replace('.ipynb', ' copy.ipynb')}
          placeholder="Enter notebook name"
          onConfirm={handleDuplicateConfirm}
          onCancel={() => setDuplicatingFile(null)}
        />
      )}
    </div>
  );
}
