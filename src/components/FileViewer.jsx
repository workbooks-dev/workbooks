import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import Editor from "@monaco-editor/react";
import { marked } from "marked";

export function FileViewer({ filePath, onClose }) {
  const [content, setContent] = useState("");
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState(null);
  const [hasUnsavedChanges, setHasUnsavedChanges] = useState(false);
  const [isEditing, setIsEditing] = useState(false);

  const isMarkdown = filePath.endsWith(".md");
  const language = getLanguageFromExtension(filePath);

  useEffect(() => {
    loadFile();
  }, [filePath]);

  useEffect(() => {
    // Global keyboard shortcuts
    const handleKeyDown = (e) => {
      // Cmd/Ctrl+S to save
      if ((e.metaKey || e.ctrlKey) && e.key === "s") {
        e.preventDefault();
        saveFile();
        return;
      }
    };

    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [content]);

  // For non-markdown files, always start in edit mode
  useEffect(() => {
    if (!isMarkdown) {
      setIsEditing(true);
    }
  }, [isMarkdown]);

  const loadFile = async () => {
    setLoading(true);
    setError(null);

    try {
      const fileContent = await invoke("read_file", {
        filePath: filePath,
      });
      setContent(fileContent);
      setHasUnsavedChanges(false);
    } catch (err) {
      console.error("Failed to load file:", err);
      setError(err.toString());
    } finally {
      setLoading(false);
    }
  };

  const saveFile = async () => {
    if (!content) return;

    try {
      await invoke("save_file", {
        filePath: filePath,
        content: content,
      });
      setHasUnsavedChanges(false);
      console.log("File saved successfully");
    } catch (err) {
      console.error("Failed to save file:", err);
      setError(err.toString());
    }
  };

  const handleEditorChange = (value) => {
    setContent(value || "");
    setHasUnsavedChanges(true);
  };

  const toggleEditMode = () => {
    setIsEditing(!isEditing);
  };

  const getFileName = () => {
    return filePath.split("/").pop();
  };

  const renderMarkdownPreview = () => {
    const html = marked.parse(content);
    return <div className="markdown-preview" dangerouslySetInnerHTML={{ __html: html }} />;
  };

  if (loading) {
    return (
      <div className="file-viewer loading">
        <p>Loading file...</p>
      </div>
    );
  }

  if (error) {
    return (
      <div className="file-viewer error">
        <p>Error: {error}</p>
        <button onClick={onClose}>Close</button>
      </div>
    );
  }

  return (
    <div className="file-viewer">
      <div className="file-header">
        <h2>
          {getFileName()}
          {hasUnsavedChanges && <span className="unsaved-indicator"> •</span>}
        </h2>
        <div className="file-actions">
          {isMarkdown && (
            <button onClick={toggleEditMode} className="toggle-mode">
              {isEditing ? "Preview" : "Edit"}
            </button>
          )}
          <button onClick={saveFile} className={hasUnsavedChanges ? "primary" : ""}>
            Save {hasUnsavedChanges && "*"}
          </button>
          <button onClick={onClose}>Close</button>
        </div>
      </div>

      <div className="file-content">
        {isMarkdown && !isEditing ? (
          renderMarkdownPreview()
        ) : (
          <Editor
            height="calc(100vh - 120px)"
            language={language}
            value={content}
            onChange={handleEditorChange}
            theme="vs-light"
            options={{
              minimap: { enabled: false },
              fontSize: 13,
              lineNumbers: "on",
              scrollBeyondLastLine: false,
              automaticLayout: true,
              wordWrap: "on",
            }}
          />
        )}
      </div>
    </div>
  );
}

function getLanguageFromExtension(filePath) {
  const ext = filePath.split(".").pop()?.toLowerCase();

  const languageMap = {
    js: "javascript",
    jsx: "javascript",
    ts: "typescript",
    tsx: "typescript",
    py: "python",
    rb: "ruby",
    java: "java",
    c: "c",
    cpp: "cpp",
    cs: "csharp",
    php: "php",
    go: "go",
    rs: "rust",
    swift: "swift",
    kt: "kotlin",
    scala: "scala",
    sh: "shell",
    bash: "shell",
    zsh: "shell",
    sql: "sql",
    json: "json",
    xml: "xml",
    html: "html",
    css: "css",
    scss: "scss",
    sass: "sass",
    less: "less",
    md: "markdown",
    yaml: "yaml",
    yml: "yaml",
    toml: "toml",
    ini: "ini",
    txt: "plaintext",
  };

  return languageMap[ext] || "plaintext";
}
