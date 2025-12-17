import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";

function FileTreeItem({ file, level = 0, onFileClick }) {
  const [isExpanded, setIsExpanded] = useState(false);
  const [children, setChildren] = useState([]);
  const [loading, setLoading] = useState(false);

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

  return (
    <>
      <div
        className={`tree-item ${file.is_dir ? 'directory' : 'file'}`}
        style={{ paddingLeft: `${level * 12 + 8}px` }}
        onClick={handleToggle}
      >
        <span className="tree-icon">{getFileIcon()}</span>
        <span className="tree-name">{file.name}</span>
        {loading && <span className="tree-loading">...</span>}
      </div>
      {isExpanded && children.length > 0 && (
        <div className="tree-children">
          {children.map((child) => (
            <FileTreeItem
              key={child.path}
              file={child}
              level={level + 1}
              onFileClick={onFileClick}
            />
          ))}
        </div>
      )}
    </>
  );
}

export function FileExplorer({ projectRoot, projectName, onOpenNotebook }) {
  const [files, setFiles] = useState([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState(null);
  const [creatingNotebook, setCreatingNotebook] = useState(false);
  const [notebookName, setNotebookName] = useState("");

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
      onOpenNotebook?.(file.path, "notebook");
    } else {
      onOpenNotebook?.(file.path, "file");
    }
  };

  const handleNewNotebook = () => {
    setCreatingNotebook(true);
    setNotebookName("");
  };

  const handleCreateNotebook = async (e) => {
    e.preventDefault();

    if (!notebookName.trim()) {
      return;
    }

    try {
      const notebooksDir = `${projectRoot}/notebooks`;
      const notebookPath = await invoke("create_notebook", {
        notebookPath: notebooksDir,
        notebookName: notebookName,
      });

      console.log("Created notebook:", notebookPath);
      setCreatingNotebook(false);
      setNotebookName("");

      // Refresh the file list
      await loadRootFiles();
    } catch (err) {
      console.error("Failed to create notebook:", err);
      setError(err.toString());
    }
  };

  const handleCancelCreate = () => {
    setCreatingNotebook(false);
    setNotebookName("");
  };

  return (
    <div className="file-explorer">
      <div className="file-explorer-header">
        <h3>{projectName}</h3>
        <button
          className="new-notebook-btn"
          onClick={handleNewNotebook}
          title="Create new notebook"
        >
          +
        </button>
      </div>

      {creatingNotebook && (
        <div className="create-notebook-form">
          <form onSubmit={handleCreateNotebook}>
            <input
              type="text"
              value={notebookName}
              onChange={(e) => setNotebookName(e.target.value)}
              placeholder="Notebook name"
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
            />
          ))}
          {files.length === 0 && (
            <div className="file-tree-empty">No files</div>
          )}
        </div>
      )}
    </div>
  );
}
