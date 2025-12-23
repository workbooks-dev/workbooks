import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import Editor from "@monaco-editor/react";
import { marked } from "marked";

export function FileViewer({ filePath, projectRoot, isDeleted, onClose, onUnsavedChangesUpdate, onFileRestored }) {
  const [content, setContent] = useState("");
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState(null);
  const [hasUnsavedChanges, setHasUnsavedChanges] = useState(false);
  const [isEditing, setIsEditing] = useState(false);
  const [fileNotFound, setFileNotFound] = useState(false);
  const [imageZoom, setImageZoom] = useState(100);
  const [imageDataUrl, setImageDataUrl] = useState("");
  const [imageDimensions, setImageDimensions] = useState({ width: 0, height: 0 });
  const [imageFileSize, setImageFileSize] = useState(0);
  const [csvData, setCsvData] = useState({ headers: [], rows: [] });
  const [csvSortColumn, setCsvSortColumn] = useState(null);
  const [csvSortDirection, setCsvSortDirection] = useState('asc');
  const [showRawCSV, setShowRawCSV] = useState(false);
  const [jsonData, setJsonData] = useState(null);
  const [jsonError, setJsonError] = useState(null);
  const [showRawJSON, setShowRawJSON] = useState(false);

  const isMarkdown = filePath.endsWith(".md");
  const isImage = isImageFile(filePath);
  const isCSV = filePath.endsWith(".csv");
  const isJSON = filePath.endsWith(".json");
  const language = getLanguageFromExtension(filePath);

  useEffect(() => {
    loadFile();
  }, [filePath]);

  // Notify parent when unsaved changes state changes
  useEffect(() => {
    if (onUnsavedChangesUpdate) {
      onUnsavedChangesUpdate(hasUnsavedChanges);
    }
  }, [hasUnsavedChanges, onUnsavedChangesUpdate]);

  // Listen for save-all event from parent
  useEffect(() => {
    const handleSaveAll = () => {
      if (hasUnsavedChanges) {
        console.log("Saving file in response to save-all event");
        saveFile();
      }
    };

    window.addEventListener("tether:save-all", handleSaveAll);
    return () => window.removeEventListener("tether:save-all", handleSaveAll);
  }, [hasUnsavedChanges]);

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
      // For images, load as binary and convert to data URL
      if (isImage) {
        const binaryData = await invoke("read_file_binary", {
          filePath: filePath,
        });

        // Store file size
        setImageFileSize(binaryData.length);

        // Determine MIME type from file extension
        const ext = filePath.split(".").pop()?.toLowerCase();
        const mimeTypes = {
          png: "image/png",
          jpg: "image/jpeg",
          jpeg: "image/jpeg",
          gif: "image/gif",
          svg: "image/svg+xml",
          webp: "image/webp",
          bmp: "image/bmp",
          ico: "image/x-icon",
        };
        const mimeType = mimeTypes[ext] || "image/png";

        // Convert byte array to data URL
        const blob = new Blob([new Uint8Array(binaryData)], { type: mimeType });
        const dataUrl = await new Promise((resolve) => {
          const reader = new FileReader();
          reader.onloadend = () => resolve(reader.result);
          reader.readAsDataURL(blob);
        });

        setImageDataUrl(dataUrl);

        // Load image to get dimensions
        const img = new Image();
        img.onload = () => {
          setImageDimensions({ width: img.naturalWidth, height: img.naturalHeight });
          setLoading(false);
        };
        img.onerror = () => {
          setLoading(false);
        };
        img.src = dataUrl;
        return;
      }

      const fileContent = await invoke("read_file", {
        filePath: filePath,
      });
      setContent(fileContent);
      setHasUnsavedChanges(false);

      // Parse CSV if applicable
      if (isCSV && fileContent) {
        const parsed = parseCSV(fileContent);
        setCsvData(parsed);
      }

      // Parse JSON if applicable
      if (isJSON && fileContent) {
        try {
          const parsed = JSON.parse(fileContent);
          setJsonData(parsed);
          setJsonError(null);
        } catch (err) {
          setJsonError(err.message);
          setJsonData(null);
        }
      }
    } catch (err) {
      console.error("Failed to load file:", err);
      // Check if it's a "file not found" error
      if (err.toString().includes("No such file") || err.toString().includes("not found") || isDeleted) {
        setFileNotFound(true);
        // Keep content in memory so user can re-save
      } else {
        setError(err.toString());
      }
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

      // If file was deleted/not found, notify parent and refresh
      if (fileNotFound || isDeleted) {
        setFileNotFound(false);
        if (onFileRestored) {
          onFileRestored();
        }
        window.dispatchEvent(new CustomEvent("tether:files-changed"));
      }

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

  const getBreadcrumbs = (projectRoot) => {
    // Get the relative path from project root
    const relativePath = filePath.replace(projectRoot, '').replace(/^\//, '');
    const parts = relativePath.split('/');

    // Create breadcrumb items with paths
    const breadcrumbs = [];
    let currentPath = projectRoot;

    parts.forEach((part, index) => {
      currentPath = index === 0 ? `${projectRoot}/${part}` : `${currentPath}/${part}`;
      breadcrumbs.push({
        label: part,
        path: currentPath,
        isLast: index === parts.length - 1
      });
    });

    return breadcrumbs;
  };

  const renderMarkdownPreview = () => {
    const html = marked.parse(content);
    return <div className="markdown-preview" dangerouslySetInnerHTML={{ __html: html }} />;
  };

  const formatFileSize = (bytes) => {
    if (bytes === 0) return "0 B";
    const k = 1024;
    const sizes = ["B", "KB", "MB", "GB"];
    const i = Math.floor(Math.log(bytes) / Math.log(k));
    return Math.round(bytes / Math.pow(k, i) * 100) / 100 + " " + sizes[i];
  };

  const renderImageViewer = () => {
    return (
      <div className="flex flex-col h-full bg-gray-50">
        {/* Image metadata and controls bar */}
        <div className="border-b border-gray-200 bg-white px-4 py-3 flex items-center justify-between">
          <div className="flex items-center gap-6 min-w-0 flex-1">
            {projectRoot ? (
              <div className="flex items-center gap-1 text-sm min-w-0">
                {getBreadcrumbs(projectRoot).map((crumb, index) => (
                  <div key={index} className="flex items-center gap-1 min-w-0">
                    <span
                      className={`overflow-hidden text-ellipsis whitespace-nowrap ${
                        crumb.isLast
                          ? 'font-medium text-gray-900'
                          : 'text-gray-500'
                      }`}
                      title={crumb.label}
                    >
                      {crumb.label}
                    </span>
                    {!crumb.isLast && (
                      <span className="text-gray-400 flex-shrink-0">/</span>
                    )}
                  </div>
                ))}
              </div>
            ) : (
              <span className="text-sm font-medium text-gray-900">{getFileName()}</span>
            )}
            {imageDimensions.width > 0 && (
              <span className="text-sm text-gray-600">
                {imageDimensions.width} × {imageDimensions.height} px
              </span>
            )}
            {imageFileSize > 0 && (
              <span className="text-sm text-gray-600">{formatFileSize(imageFileSize)}</span>
            )}
          </div>
          <div className="flex items-center gap-3">
            <button
              onClick={() => setImageZoom(Math.max(25, imageZoom - 25))}
              className="px-3 py-1.5 text-sm bg-white border border-gray-300 rounded hover:bg-gray-50 transition-colors disabled:opacity-50 disabled:cursor-not-allowed"
              disabled={imageZoom <= 25}
            >
              −
            </button>
            <span className="text-sm font-medium text-gray-700 min-w-[60px] text-center">{imageZoom}%</span>
            <button
              onClick={() => setImageZoom(Math.min(400, imageZoom + 25))}
              className="px-3 py-1.5 text-sm bg-white border border-gray-300 rounded hover:bg-gray-50 transition-colors disabled:opacity-50 disabled:cursor-not-allowed"
              disabled={imageZoom >= 400}
            >
              +
            </button>
            <button
              onClick={() => setImageZoom(100)}
              className="px-3 py-1.5 text-sm bg-blue-600 text-white rounded hover:bg-blue-700 transition-colors"
            >
              Actual Size
            </button>
            <div className="w-px h-6 bg-gray-300 mx-1"></div>
            <button
              onClick={onClose}
              className="px-3 py-1.5 text-sm bg-white border border-gray-300 rounded hover:bg-gray-50 transition-colors"
            >
              Close
            </button>
          </div>
        </div>

        {/* Image display area */}
        <div className="flex-1 overflow-auto flex items-center justify-center p-8">
          {imageDataUrl ? (
            <img
              src={imageDataUrl}
              alt={getFileName()}
              style={{ width: `${imageZoom}%`, maxWidth: 'none' }}
              className="shadow-lg"
            />
          ) : (
            <div className="text-gray-500">Loading image...</div>
          )}
        </div>
      </div>
    );
  };

  const handleCSVSort = (columnIndex) => {
    if (csvSortColumn === columnIndex) {
      setCsvSortDirection(csvSortDirection === 'asc' ? 'desc' : 'asc');
    } else {
      setCsvSortColumn(columnIndex);
      setCsvSortDirection('asc');
    }
  };

  const getSortedCSVRows = () => {
    if (csvSortColumn === null) return csvData.rows;

    return [...csvData.rows].sort((a, b) => {
      const aVal = a[csvSortColumn] || '';
      const bVal = b[csvSortColumn] || '';

      // Try numeric comparison first
      const aNum = parseFloat(aVal);
      const bNum = parseFloat(bVal);

      if (!isNaN(aNum) && !isNaN(bNum)) {
        return csvSortDirection === 'asc' ? aNum - bNum : bNum - aNum;
      }

      // Fall back to string comparison
      return csvSortDirection === 'asc'
        ? aVal.localeCompare(bVal)
        : bVal.localeCompare(aVal);
    });
  };

  const renderCSVViewer = () => {
    const sortedRows = getSortedCSVRows();
    const displayRows = sortedRows.slice(0, 1000); // Limit to first 1000 rows for performance

    return (
      <div className="h-full flex flex-col bg-white">
        <div className="border-b border-gray-200 px-4 py-2 flex items-center justify-between bg-gray-50">
          <div className="text-sm text-gray-600">
            {csvData.rows.length} rows × {csvData.headers.length} columns
            {csvData.rows.length > 1000 && <span className="text-amber-600 ml-2">(showing first 1000)</span>}
          </div>
          <button
            onClick={() => setShowRawCSV(!showRawCSV)}
            className="px-3 py-1.5 text-sm bg-white border border-gray-300 rounded hover:bg-gray-50 transition-colors"
          >
            {showRawCSV ? 'Table View' : 'Raw CSV'}
          </button>
        </div>

        {showRawCSV ? (
          <Editor
            height="calc(100vh - 180px)"
            language="plaintext"
            value={content}
            onChange={handleEditorChange}
            theme="vs-light"
            options={{
              minimap: { enabled: false },
              fontSize: 13,
              lineNumbers: "on",
              scrollBeyondLastLine: false,
              automaticLayout: true,
              wordWrap: "off",
            }}
          />
        ) : (
          <div className="flex-1 overflow-auto">
            <table className="w-full text-sm border-collapse">
              <thead className="sticky top-0 bg-gray-100 border-b border-gray-300">
                <tr>
                  <th className="px-4 py-2 text-left text-xs font-semibold text-gray-600 border-r border-gray-300 bg-gray-200 w-12">
                    #
                  </th>
                  {csvData.headers.map((header, idx) => (
                    <th
                      key={idx}
                      onClick={() => handleCSVSort(idx)}
                      className="px-4 py-2 text-left text-xs font-semibold text-gray-700 border-r border-gray-200 cursor-pointer hover:bg-gray-200 transition-colors"
                    >
                      <div className="flex items-center gap-2">
                        <span>{header || `Column ${idx + 1}`}</span>
                        {csvSortColumn === idx && (
                          <span className="text-blue-600">
                            {csvSortDirection === 'asc' ? '↑' : '↓'}
                          </span>
                        )}
                      </div>
                    </th>
                  ))}
                </tr>
              </thead>
              <tbody>
                {displayRows.map((row, rowIdx) => (
                  <tr key={rowIdx} className="border-b border-gray-200 hover:bg-blue-50 transition-colors">
                    <td className="px-4 py-2 text-xs text-gray-500 border-r border-gray-300 bg-gray-50">
                      {rowIdx + 1}
                    </td>
                    {row.map((cell, cellIdx) => (
                      <td key={cellIdx} className="px-4 py-2 text-gray-900 border-r border-gray-100">
                        {cell}
                      </td>
                    ))}
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )}
      </div>
    );
  };

  const renderJSONViewer = () => {
    if (jsonError) {
      return (
        <div className="h-full flex flex-col bg-white">
          <div className="border-b border-gray-200 px-4 py-2 bg-red-50">
            <div className="text-sm text-red-700 font-medium">Invalid JSON: {jsonError}</div>
          </div>
          <Editor
            height="calc(100vh - 180px)"
            language="json"
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
        </div>
      );
    }

    return (
      <div className="h-full flex flex-col bg-white">
        <div className="border-b border-gray-200 px-4 py-2 flex items-center justify-between bg-gray-50">
          <div className="text-sm text-gray-600">JSON Document</div>
          <button
            onClick={() => setShowRawJSON(!showRawJSON)}
            className="px-3 py-1.5 text-sm bg-white border border-gray-300 rounded hover:bg-gray-50 transition-colors"
          >
            {showRawJSON ? 'Tree View' : 'Raw JSON'}
          </button>
        </div>

        {showRawJSON ? (
          <Editor
            height="calc(100vh - 180px)"
            language="json"
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
        ) : (
          <div className="flex-1 overflow-auto p-4 font-mono text-sm">
            <JSONNode data={jsonData} name="root" level={0} />
          </div>
        )}
      </div>
    );
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

  // Determine if the current view is read-only
  const isReadOnly = isImage || (isCSV && !showRawCSV) || (isJSON && !showRawJSON);

  return (
    <div className="flex flex-col h-full">
      {/* Deleted file banner */}
      {(fileNotFound || isDeleted) && (
        <div className="bg-red-50 border-b border-red-200 px-4 py-3 flex items-center justify-between">
          <div className="flex items-center gap-2">
            <svg className="w-5 h-5 text-red-600" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M12 9v2m0 4h.01m-6.938 4h13.856c1.54 0 2.502-1.667 1.732-3L13.732 4c-.77-1.333-2.694-1.333-3.464 0L3.34 16c-.77 1.333.192 3 1.732 3z" />
            </svg>
            <span className="text-sm font-medium text-red-900">
              This file has been deleted or moved. You can still save it to restore it.
            </span>
          </div>
          <button
            onClick={saveFile}
            className="px-3 py-1.5 text-sm font-medium text-white bg-red-600 hover:bg-red-700 rounded transition-colors"
          >
            Save to Restore
          </button>
        </div>
      )}

      {/* Header for images is built into the viewer */}
      {!isImage && (
        <div className="border-b border-gray-200 bg-white px-4 py-3 flex items-center justify-between flex-shrink-0">
          <div className="flex items-center gap-1 text-sm min-w-0 flex-1">
            {projectRoot && getBreadcrumbs(projectRoot).map((crumb, index, arr) => (
              <div key={index} className="flex items-center gap-1 min-w-0">
                <span
                  className={`overflow-hidden text-ellipsis whitespace-nowrap ${
                    crumb.isLast
                      ? 'font-medium text-gray-900'
                      : 'text-gray-500'
                  }`}
                  title={crumb.label}
                >
                  {crumb.label}
                </span>
                {!crumb.isLast && (
                  <span className="text-gray-400 flex-shrink-0">/</span>
                )}
              </div>
            ))}
            {!projectRoot && (
              <span className="font-medium text-gray-900">{getFileName()}</span>
            )}
            {hasUnsavedChanges && <span className="text-blue-600 ml-1 flex-shrink-0">•</span>}
          </div>
          <div className="flex items-center gap-2">
            {isMarkdown && (
              <button
                onClick={toggleEditMode}
                className="px-3 py-1.5 text-sm bg-white border border-gray-300 rounded hover:bg-gray-50 transition-colors"
              >
                {isEditing ? "Preview" : "Edit"}
              </button>
            )}
            {!isReadOnly && (
              <button
                onClick={saveFile}
                className={`px-3 py-1.5 text-sm rounded transition-colors ${
                  hasUnsavedChanges
                    ? "bg-blue-600 text-white hover:bg-blue-700"
                    : "bg-white border border-gray-300 hover:bg-gray-50"
                }`}
              >
                Save {hasUnsavedChanges && "*"}
              </button>
            )}
            <button
              onClick={onClose}
              className="px-3 py-1.5 text-sm bg-white border border-gray-300 rounded hover:bg-gray-50 transition-colors"
            >
              Close
            </button>
          </div>
        </div>
      )}

      <div className="flex-1 overflow-hidden">
        {isImage ? (
          renderImageViewer()
        ) : isCSV ? (
          renderCSVViewer()
        ) : isJSON ? (
          renderJSONViewer()
        ) : isMarkdown && !isEditing ? (
          renderMarkdownPreview()
        ) : (
          <Editor
            height="100%"
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

function isImageFile(filePath) {
  const ext = filePath.split(".").pop()?.toLowerCase();
  const imageExtensions = ["png", "jpg", "jpeg", "gif", "svg", "webp", "bmp", "ico"];
  return imageExtensions.includes(ext);
}

function parseCSV(csvText) {
  const lines = csvText.split('\n').filter(line => line.trim());

  if (lines.length === 0) {
    return { headers: [], rows: [] };
  }

  // Simple CSV parser - handles basic comma-separated values
  // For production, consider using a library like PapaParse for complex cases
  const parseLine = (line) => {
    const result = [];
    let current = '';
    let inQuotes = false;

    for (let i = 0; i < line.length; i++) {
      const char = line[i];

      if (char === '"') {
        inQuotes = !inQuotes;
      } else if (char === ',' && !inQuotes) {
        result.push(current.trim());
        current = '';
      } else {
        current += char;
      }
    }
    result.push(current.trim());
    return result;
  };

  const headers = parseLine(lines[0]);
  const rows = lines.slice(1).map(line => parseLine(line));

  return { headers, rows };
}

function JSONNode({ data, name, level }) {
  const [isExpanded, setIsExpanded] = useState(level < 2);

  const getDataType = (value) => {
    if (value === null) return 'null';
    if (Array.isArray(value)) return 'array';
    return typeof value;
  };

  const getValueColor = (type) => {
    switch (type) {
      case 'string': return 'text-green-700';
      case 'number': return 'text-blue-700';
      case 'boolean': return 'text-purple-700';
      case 'null': return 'text-gray-500';
      default: return 'text-gray-900';
    }
  };

  const type = getDataType(data);
  const isExpandable = type === 'object' || type === 'array';

  const renderValue = () => {
    if (type === 'string') return `"${data}"`;
    if (type === 'null') return 'null';
    if (type === 'boolean') return data.toString();
    if (type === 'number') return data.toString();
    return '';
  };

  const getPreview = () => {
    if (type === 'object') {
      const keys = Object.keys(data);
      return `{ ${keys.length} ${keys.length === 1 ? 'key' : 'keys'} }`;
    }
    if (type === 'array') {
      return `[ ${data.length} ${data.length === 1 ? 'item' : 'items'} ]`;
    }
    return '';
  };

  if (!isExpandable) {
    return (
      <div className="py-0.5" style={{ paddingLeft: `${level * 16}px` }}>
        <span className="text-blue-600 font-medium">{name}</span>
        <span className="text-gray-500">: </span>
        <span className={getValueColor(type)}>{renderValue()}</span>
      </div>
    );
  }

  return (
    <div style={{ paddingLeft: `${level * 16}px` }}>
      <div
        className="py-0.5 cursor-pointer hover:bg-gray-100 inline-block"
        onClick={() => setIsExpanded(!isExpanded)}
      >
        <span className="text-gray-400 mr-1 w-4 inline-block">
          {isExpanded ? '▼' : '▶'}
        </span>
        <span className="text-blue-600 font-medium">{name}</span>
        <span className="text-gray-500">: </span>
        {!isExpanded && <span className="text-gray-400 italic">{getPreview()}</span>}
      </div>
      {isExpanded && (
        <div>
          {type === 'array' ? (
            data.map((item, idx) => (
              <JSONNode key={idx} data={item} name={`[${idx}]`} level={level + 1} />
            ))
          ) : (
            Object.entries(data).map(([key, value]) => (
              <JSONNode key={key} data={value} name={key} level={level + 1} />
            ))
          )}
        </div>
      )}
    </div>
  );
}
