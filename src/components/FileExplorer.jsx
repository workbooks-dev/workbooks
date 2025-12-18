import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { ask } from "@tauri-apps/plugin-dialog";
import { ContextMenu } from "./ContextMenu";
import { InputDialog } from "./InputDialog";

function FileTreeItem({ file, level = 0, onFileClick, onFileAction }) {
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

  return (
    <>
      <div
        className={`tree-item ${file.is_dir ? 'directory' : 'file'}`}
        style={{ paddingLeft: `${level * 12 + 8}px` }}
        onClick={handleToggle}
        onContextMenu={handleContextMenu}
      >
        <span className="tree-icon">{getFileIcon()}</span>
        <span className="tree-name">{file.name}</span>
        {loading && <span className="tree-loading">...</span>}
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
            />
          ))}
        </div>
      )}
    </>
  );
}

export function FileExplorer({ projectRoot, projectName, onOpenWorkbook, onFileDeleted }) {
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
    <div className="file-explorer">
      <div className="file-explorer-header">
        <h3>{projectName}</h3>
        <button
          className="new-workbook-btn"
          onClick={handleNewWorkbook}
          title="Create new workbook"
        >
          +
        </button>
      </div>

      {creatingWorkbook && (
        <div className="create-workbook-form">
          <form onSubmit={handleCreateWorkbook}>
            <input
              type="text"
              value={workbookName}
              onChange={(e) => setWorkbookName(e.target.value)}
              placeholder="Workbook name"
              autoFocus
            />
            <div className="form-actions">
              <button type="submit">Create</button>
              <button type="button" onClick={handleCancelCreate}>Cancel</button>
            </div>
          </form>
        </div>
      )}

      {loading && <div className="file-explorer-loading">Loading...</div>}

      {error && (
        <div className="file-explorer-error">
          {error}
        </div>
      )}

      {!loading && !error && (
        <div className="file-tree">
          {files.map((file) => (
            <FileTreeItem
              key={file.path}
              file={file}
              level={0}
              onFileClick={handleFileClick}
              onFileAction={handleFileAction}
            />
          ))}
          {files.length === 0 && (
            <div className="file-tree-empty">No files</div>
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
