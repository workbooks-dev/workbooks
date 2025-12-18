import { useState, useEffect, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import { ask } from "@tauri-apps/plugin-dialog";
import { listen } from "@tauri-apps/api/event";
import Editor from "@monaco-editor/react";
import ReactMarkdown from "react-markdown";
import { Prism as SyntaxHighlighter } from "react-syntax-highlighter";
import { vscDarkPlus } from "react-syntax-highlighter/dist/esm/styles/prism";

// Hash function to match Rust implementation in lib.rs
function hashString(s) {
  let hash = 0;
  for (let i = 0; i < s.length; i++) {
    hash = ((hash << 5) - hash) + s.charCodeAt(i);
    hash = hash & hash; // Convert to 32-bit integer
  }
  return Math.abs(hash);
}

function WorkbookCell({ cell, index, onUpdate, onDelete, onExecute, onMoveUp, onMoveDown, onClearOutput, isSelected, isEditMode, isRunning, executionElapsed, onSelect, onEnterEditMode, onInsertBelow, autosaveEnabled }) {
  // Initialize content from cell source ONCE on mount - don't sync after that
  const [content, setContent] = useState(cell.source.join(""));
  const editorRef = useRef(null);

  // Auto-focus editor when cell enters edit mode
  useEffect(() => {
    if (isEditMode && isSelected && editorRef.current) {
      // Small delay to ensure DOM is ready
      setTimeout(() => {
        editorRef.current?.focus();
      }, 50);
    }
  }, [isEditMode, isSelected]);

  const handleEditorChange = (value) => {
    setContent(value || "");
  };

  const handleExecute = (mode = 'shift-enter') => {
    // Get the current value directly from the editor to avoid stale state
    const currentValue = editorRef.current ? editorRef.current.getValue() : content;

    // Update local state to match (though it might already be updating)
    if (currentValue !== content) {
      setContent(currentValue);
    }

    // Update the cell source before executing
    onUpdate(index, currentValue);
    onExecute(index, currentValue, mode);
  };

  const handleBlur = () => {
    // Blur no longer auto-saves - user must explicitly save via Cmd+S or Save button
  };

  const handleKeyDown = (e) => {
    if (e.shiftKey && e.key === "Enter") {
      e.preventDefault();
      handleExecute('shift-enter');
    } else if ((e.ctrlKey || e.metaKey) && e.key === "Enter") {
      e.preventDefault();
      handleExecute('ctrl-enter');
    } else if (e.altKey && e.key === "Enter") {
      e.preventDefault();
      handleExecute('alt-enter');
    }
  };

  // No longer auto-update on edit mode change - user must explicitly save

  if (cell.cell_type === "markdown") {
    return (
      <div
        className={`notebook-cell markdown-cell ${isSelected ? "selected" : ""} ${isEditMode ? "edit-mode" : ""}`}
        onClick={() => onSelect(index)}
        onDoubleClick={() => {
          onSelect(index);
          onEnterEditMode();
        }}
      >
        {isSelected && (
          <div className="cell-toolbar">
            <button onClick={(e) => { e.stopPropagation(); handleExecute(); }} title="Run cell (render)">▶</button>
            <button onClick={(e) => { e.stopPropagation(); onMoveUp(index); }} title="Move up">↑</button>
            <button onClick={(e) => { e.stopPropagation(); onMoveDown(index); }} title="Move down">↓</button>
            <button onClick={(e) => { e.stopPropagation(); onDelete(index); }} title="Delete">🗑</button>
          </div>
        )}
        <div className="cell-input">
          {isEditMode ? (
            <textarea
              value={content}
              onChange={(e) => handleEditorChange(e.target.value)}
              onKeyDown={handleKeyDown}
              onBlur={handleBlur}
              className="markdown-editor"
              placeholder="Enter markdown..."
              rows={Math.max(3, content.split("\n").length)}
              autoFocus
            />
          ) : (
            <div className="markdown-rendered">
              {content ? (
                <ReactMarkdown
                  components={{
                    code({ node, inline, className, children, ...props }) {
                      const match = /language-(\w+)/.exec(className || '');
                      return !inline && match ? (
                        <SyntaxHighlighter
                          style={vscDarkPlus}
                          language={match[1]}
                          PreTag="div"
                          {...props}
                        >
                          {String(children).replace(/\n$/, '')}
                        </SyntaxHighlighter>
                      ) : (
                        <code className={className} {...props}>
                          {children}
                        </code>
                      );
                    }
                  }}
                >
                  {content}
                </ReactMarkdown>
              ) : (
                <div className="markdown-placeholder">Double-click to edit markdown</div>
              )}
            </div>
          )}
        </div>
      </div>
    );
  }

  if (cell.cell_type === "code") {
    const outputs = cell.outputs || [];
    const hasOutput = outputs.length > 0;

    return (
      <div
        className={`notebook-cell code-cell ${isSelected ? "selected" : ""} ${isEditMode ? "edit-mode" : ""}`}
        onClick={() => onSelect(index)}
      >
        {isSelected && (
          <div className="cell-toolbar">
            <button onClick={(e) => { e.stopPropagation(); handleExecute(); }} title="Run cell">▶</button>
            {hasOutput && (
              <button onClick={(e) => { e.stopPropagation(); onClearOutput(index); }} title="Clear output">🗙</button>
            )}
            <button onClick={(e) => { e.stopPropagation(); onMoveUp(index); }} title="Move up">↑</button>
            <button onClick={(e) => { e.stopPropagation(); onMoveDown(index); }} title="Move down">↓</button>
            <button onClick={(e) => { e.stopPropagation(); onDelete(index); }} title="Delete">🗑</button>
          </div>
        )}
        <div className="cell-prompt">
          [{cell.execution_count || " "}]
          {isRunning && executionElapsed > 0 && (
            <div className="execution-timer" title="Execution time">
              {(executionElapsed / 1000).toFixed(1)}s
            </div>
          )}
        </div>
        <div className="cell-container">
          <div className="cell-input">
            <Editor
              height={`${Math.max(100, content.split("\n").length * 19)}px`}
              defaultLanguage="python"
              value={content}
              onChange={handleEditorChange}
              onMount={(editor) => {
                editorRef.current = editor;

                // Focus editor if cell is in edit mode when it mounts
                if (isEditMode && isSelected) {
                  setTimeout(() => {
                    editor.focus();
                  }, 50);
                }

                editor.onKeyDown((e) => {
                  if (e.shiftKey && e.keyCode === 3) {
                    // Shift+Enter (keyCode 3 is Enter in Monaco)
                    e.preventDefault();
                    handleExecute('shift-enter');
                  } else if ((e.ctrlKey || e.metaKey) && e.keyCode === 3) {
                    // Ctrl/Cmd+Enter
                    e.preventDefault();
                    handleExecute('ctrl-enter');
                  } else if (e.altKey && e.keyCode === 3) {
                    // Alt+Enter
                    e.preventDefault();
                    handleExecute('alt-enter');
                  }
                });
                editor.onDidBlurEditorText(() => {
                  handleBlur();
                });
              }}
              theme="vs-light"
              options={{
                minimap: { enabled: false },
                fontSize: 13,
                lineNumbers: "on",
                scrollBeyondLastLine: false,
                automaticLayout: true,
                wordWrap: "on",
                scrollbar: {
                  vertical: "hidden",
                  horizontal: "hidden",
                },
              }}
            />
          </div>
          {hasOutput && (
            <div className="cell-outputs">
              {outputs.map((output, idx) => (
                <CellOutput key={idx} output={output} />
              ))}
            </div>
          )}
        </div>
      </div>
    );
  }

  return null;
}

function CellOutput({ output }) {
  const [expanded, setExpanded] = useState(false);
  const MAX_LINES = 20; // Show first 20 lines by default

  // Check if output was truncated by the backend
  const isTruncatedByBackend = output.metadata && output.metadata.truncated === true;

  // Strip ANSI color codes from text
  const stripAnsi = (text) => {
    if (!text) return text;
    // Remove ANSI escape sequences
    return text.replace(/\x1b\[[0-9;]*m/g, '');
  };

  const truncateText = (text, maxLines) => {
    const lines = text.split('\n');
    if (lines.length <= maxLines) {
      return { text, truncated: false, totalLines: lines.length };
    }
    return {
      text: lines.slice(0, maxLines).join('\n'),
      truncated: true,
      totalLines: lines.length
    };
  };

  if (output.output_type === "stream") {
    const text = Array.isArray(output.text) ? output.text.join("") : output.text;
    const className = output.name === "stderr" ? "output-stderr" : "output-stdout";
    const cleanText = stripAnsi(text);

    const { text: displayText, truncated, totalLines } = expanded
      ? { text: cleanText, truncated: false, totalLines: 0 }
      : truncateText(cleanText, MAX_LINES);

    return (
      <div className={`cell-output-wrapper ${className}`}>
        <div className="cell-output-content">
          {isTruncatedByBackend && (
            <div className="output-truncation-warning">
              ⚠ Output was truncated to save memory (max 1000 lines or 100KB per cell)
            </div>
          )}
          <pre>{displayText}</pre>
        </div>
        {truncated && (
          <button
            className="output-expand-btn"
            onClick={() => setExpanded(!expanded)}
          >
            Show more ({totalLines - MAX_LINES} more lines)
          </button>
        )}
        {expanded && (
          <button
            className="output-expand-btn"
            onClick={() => setExpanded(false)}
          >
            Show less
          </button>
        )}
      </div>
    );
  }

  // Handle both execute_result and display_data (both can have rich outputs)
  if (output.output_type === "execute_result" || output.output_type === "display_data") {
    const data = output.data || {};

    // Priority order: richest format first
    // Images (PNG)
    if (data["image/png"]) {
      return (
        <div className="cell-output-wrapper output-result">
          <div className="cell-output-content">
            <img
              src={`data:image/png;base64,${data["image/png"]}`}
              alt="Output"
              style={{ maxWidth: '100%', height: 'auto' }}
            />
          </div>
        </div>
      );
    }

    // Images (JPEG)
    if (data["image/jpeg"]) {
      return (
        <div className="cell-output-wrapper output-result">
          <div className="cell-output-content">
            <img
              src={`data:image/jpeg;base64,${data["image/jpeg"]}`}
              alt="Output"
              style={{ maxWidth: '100%', height: 'auto' }}
            />
          </div>
        </div>
      );
    }

    // SVG
    if (data["image/svg+xml"]) {
      const svgContent = Array.isArray(data["image/svg+xml"])
        ? data["image/svg+xml"].join("")
        : data["image/svg+xml"];
      return (
        <div className="cell-output-wrapper output-result">
          <div className="cell-output-content">
            {/* Note: SVG content is rendered directly. For untrusted notebooks, consider sandboxing. */}
            <div dangerouslySetInnerHTML={{ __html: svgContent }} />
          </div>
        </div>
      );
    }

    // HTML (DataFrames, matplotlib HTML output, etc.)
    if (data["text/html"]) {
      const htmlContent = Array.isArray(data["text/html"])
        ? data["text/html"].join("")
        : data["text/html"];
      return (
        <div className="cell-output-wrapper output-result">
          <div className="cell-output-content">
            {/* Note: HTML rendering uses dangerouslySetInnerHTML.
                For untrusted notebooks, consider implementing a trust model or HTML sanitization. */}
            <div dangerouslySetInnerHTML={{ __html: htmlContent }} />
          </div>
        </div>
      );
    }

    // Fallback to text/plain
    if (data["text/plain"]) {
      const text = Array.isArray(data["text/plain"])
        ? data["text/plain"].join("")
        : data["text/plain"];
      const cleanText = stripAnsi(text);
      const { text: displayText, truncated, totalLines } = expanded
        ? { text: cleanText, truncated: false, totalLines: 0 }
        : truncateText(cleanText, MAX_LINES);

      return (
        <div className="cell-output-wrapper output-result">
          <div className="cell-output-content">
            {isTruncatedByBackend && (
              <div className="output-truncation-warning">
                ⚠ Output was truncated to save memory (max 1000 lines or 100KB per cell)
              </div>
            )}
            <pre>{displayText}</pre>
          </div>
          {truncated && (
            <button
              className="output-expand-btn"
              onClick={() => setExpanded(!expanded)}
            >
              Show more ({totalLines - MAX_LINES} more lines)
            </button>
          )}
          {expanded && (
            <button
              className="output-expand-btn"
              onClick={() => setExpanded(false)}
            >
              Show less
            </button>
          )}
        </div>
      );
    }

    // If no recognized mime type, show raw data
    return (
      <div className="cell-output-wrapper output-result">
        <div className="cell-output-content">
          <pre>{JSON.stringify(data, null, 2)}</pre>
        </div>
      </div>
    );
  }

  if (output.output_type === "error") {
    const traceback = output.traceback ? output.traceback.join("\n") : output.evalue;
    const cleanText = stripAnsi(traceback);
    const { text: displayText, truncated, totalLines } = expanded
      ? { text: cleanText, truncated: false, totalLines: 0 }
      : truncateText(cleanText, MAX_LINES);

    return (
      <div className="cell-output-wrapper output-error">
        <div className="cell-output-content">
          {isTruncatedByBackend && (
            <div className="output-truncation-warning">
              ⚠ Output was truncated to save memory (max 1000 lines or 100KB per cell)
            </div>
          )}
          <pre>{displayText}</pre>
        </div>
        {truncated && (
          <button
            className="output-expand-btn"
            onClick={() => setExpanded(!expanded)}
          >
            Show more ({totalLines - MAX_LINES} more lines)
          </button>
        )}
        {expanded && (
          <button
            className="output-expand-btn"
            onClick={() => setExpanded(false)}
          >
            Show less
          </button>
        )}
      </div>
    );
  }

  return null;
}

export function WorkbookViewer({ workbookPath, projectRoot, autosaveEnabled = true, onClose }) {
  const [notebook, setNotebook] = useState(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState(null);
  const [hasUnsavedChanges, setHasUnsavedChanges] = useState(false);
  const [selectedCell, setSelectedCell] = useState(0);
  const [editMode, setEditMode] = useState(false);
  const [isRunningAll, setIsRunningAll] = useState(false);
  const [runningCellIndex, setRunningCellIndex] = useState(null);
  const [cellExecutionStartTime, setCellExecutionStartTime] = useState(null); // Track when cell execution started
  const [cellExecutionElapsed, setCellExecutionElapsed] = useState(0); // Track elapsed time in ms
  const [engineStatus, setEngineStatus] = useState('starting'); // 'starting', 'idle', 'busy', 'error', 'restarting'
  const [engineReady, setEngineReady] = useState(false);
  const engineStartedRef = useRef(false);
  const cellRefs = useRef([]);
  const currentUnlistenRef = useRef(null);
  const executingCellRef = useRef(null);
  const lastDKeyPressRef = useRef(0); // Track last 'D' key press for double-tap delete
  const loadedContentHashRef = useRef(null); // Track file content hash to detect external changes
  const executionTimerRef = useRef(null); // Timer for updating elapsed time
  const outputListenerRef = useRef(null); // Track active output listener to prevent duplicates

  useEffect(() => {
    loadWorkbook();
    startEngine();

    return () => {
      // Cleanup: stop engine and event listener when component unmounts
      if (currentUnlistenRef.current) {
        currentUnlistenRef.current();
        currentUnlistenRef.current = null;
      }
      stopEngine();
    };
  }, [workbookPath]);

  // Autosave removed - user must explicitly save via Cmd+S or Save button

  useEffect(() => {
    // Global keyboard shortcuts
    const handleKeyDown = (e) => {
      // Cmd/Ctrl+S to save
      if ((e.metaKey || e.ctrlKey) && e.key === "s") {
        e.preventDefault();
        saveWorkbook();
        return;
      }

      // Only handle navigation if not in edit mode
      if (!editMode) {
        // Arrow up/down to navigate cells
        if (e.key === "ArrowUp") {
          e.preventDefault();
          setSelectedCell(Math.max(0, selectedCell - 1));
        } else if (e.key === "ArrowDown") {
          e.preventDefault();
          setSelectedCell(Math.min(notebook?.cells?.length - 1 || 0, selectedCell + 1));
        }
        // Enter to edit cell
        else if (e.key === "Enter") {
          e.preventDefault();
          setEditMode(true);
        }
        // A to add cell above
        else if (e.key === "a") {
          e.preventDefault();
          addCellAt(selectedCell, "code");
        }
        // B to add cell below
        else if (e.key === "b") {
          e.preventDefault();
          addCellAt(selectedCell + 1, "code");
        }
        // DD (double-tap D) to delete cell (like Jupyter)
        else if (e.key === "d") {
          e.preventDefault();
          const now = Date.now();
          const timeSinceLastD = now - lastDKeyPressRef.current;

          // If D was pressed within 500ms, delete the cell
          if (timeSinceLastD < 500) {
            deleteCell(selectedCell);
            lastDKeyPressRef.current = 0; // Reset to prevent triple-tap
          } else {
            // First D press, just record the time
            lastDKeyPressRef.current = now;
          }
        }
        // M to change to markdown
        else if (e.key === "m") {
          e.preventDefault();
          changeCellType(selectedCell, "markdown");
        }
        // Y to change to code
        else if (e.key === "y") {
          e.preventDefault();
          changeCellType(selectedCell, "code");
        }
      }
      // Escape to exit edit mode
      if (editMode && e.key === "Escape") {
        e.preventDefault();
        setEditMode(false);
      }
    };

    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [notebook, selectedCell, editMode]);

  const startEngine = async () => {
    // Prevent multiple simultaneous starts
    if (engineStartedRef.current) {
      return;
    }

    try {
      setEngineStatus('starting');

      await invoke("start_engine", {
        workbookPath: workbookPath,
        projectPath: projectRoot,
        engineName: null,  // Auto-detect
      });

      engineStartedRef.current = true;
      setEngineReady(true);
      setEngineStatus('idle');

    } catch (err) {
      console.error("Failed to start engine:", err);
      setEngineStatus('error');
      engineStartedRef.current = false;
      setEngineReady(false);
    }
  };

  const stopEngine = async () => {
    try {
      engineStartedRef.current = false;
      setEngineReady(false);
      await invoke("stop_engine", {
        workbookPath: workbookPath,
      });
    } catch (err) {
      console.error("Failed to stop engine:", err);
    }
  };

  const restartEngine = async () => {
    try {
      setEngineStatus('restarting');
      setError(null);

      try {
        // First try to restart using the restart endpoint
        await invoke("restart_engine", {
          workbookPath: workbookPath,
          projectPath: projectRoot,
        });
      } catch (restartErr) {
        // If restart fails, try stopping and starting manually
        try {
          await stopEngine();
        } catch (stopErr) {
          // Ignore errors from stop
        }
        // Start a fresh engine
        await invoke("start_engine", {
          workbookPath: workbookPath,
          projectPath: projectRoot,
          engineName: null,
        });
      }

      engineStartedRef.current = true;
      setEngineReady(true);
      setEngineStatus('idle');

      // Clear all outputs after restart
      clearAllOutputs();
    } catch (err) {
      console.error("Failed to restart engine:", err);
      setEngineStatus('error');
      engineStartedRef.current = false;
      setEngineReady(false);
      setError(`Failed to restart engine: ${err}. Try using 'Reconnect Engine'.`);
    }
  };

  const interruptExecution = async () => {
    try {
      await invoke("interrupt_engine", {
        workbookPath: workbookPath,
      });
      setEngineStatus('idle');
      setRunningCellIndex(null);
    } catch (err) {
      console.error("Failed to interrupt kernel:", err);
      setError(`Failed to interrupt: ${err}`);
    }
  };

  const loadWorkbook = async () => {
    setLoading(true);
    setError(null);

    try {
      const content = await invoke("read_workbook", {
        workbookPath: workbookPath,
      });
      const parsed = JSON.parse(content);
      setNotebook(parsed);
      setHasUnsavedChanges(false);

      // Save hash of loaded content to detect external changes
      loadedContentHashRef.current = hashNotebookContent(content);
    } catch (err) {
      console.error("Failed to load notebook:", err);
      setError(err.toString());
    } finally {
      setLoading(false);
    }
  };

  // Simple hash function for content comparison
  const hashNotebookContent = (content) => {
    let hash = 0;
    for (let i = 0; i < content.length; i++) {
      const char = content.charCodeAt(i);
      hash = ((hash << 5) - hash) + char;
      hash = hash & hash;
    }
    return hash;
  };

  const saveWorkbook = async () => {
    if (!notebook) return;

    try {
      // Check if file was modified externally
      const currentFileContent = await invoke("read_workbook", {
        workbookPath: workbookPath,
      });
      const currentHash = hashNotebookContent(currentFileContent);

      if (currentHash !== loadedContentHashRef.current) {
        // File was modified externally
        const shouldOverwrite = await ask(
          "The notebook file has been modified outside of Tether. Do you want to overwrite it with your current changes?",
          {
            title: "File Modified Externally",
            kind: "warning",
            okLabel: "Overwrite",
            cancelLabel: "Cancel"
          }
        );

        if (!shouldOverwrite) {
          return;
        }
      }

      // Make a copy to ensure we have the latest state
      const notebookToSave = {
        ...notebook,
        cells: notebook.cells.map(cell => ({
          ...cell,
          // Ensure source is always an array of strings
          source: Array.isArray(cell.source) ? cell.source : [cell.source],
        })),
      };

      const content = JSON.stringify(notebookToSave, null, 2);
      await invoke("save_workbook", {
        workbookPath: workbookPath,
        content: content,
      });

      // Update hash after successful save
      loadedContentHashRef.current = hashNotebookContent(content);

      setHasUnsavedChanges(false);
    } catch (err) {
      console.error("Failed to save notebook:", err);
      setError(err.toString());
    }
  };

  const updateCell = (index, newContent) => {
    // Use functional update to prevent race conditions
    setNotebook(prevNotebook => {
      const newCells = [...prevNotebook.cells];
      newCells[index] = {
        ...newCells[index],
        source: newContent.split("\n").map((line, i, arr) =>
          i < arr.length - 1 ? line + "\n" : line
        ),
      };
      return { ...prevNotebook, cells: newCells };
    });

    setHasUnsavedChanges(true);
  };

  const deleteCell = (index) => {
    if (notebook.cells.length === 1) {
      // Don't delete the last cell
      return;
    }

    const newCellCount = notebook.cells.length - 1;

    setNotebook(prevNotebook => {
      const newCells = prevNotebook.cells.filter((_, i) => i !== index);
      return { ...prevNotebook, cells: newCells };
    });

    // Adjust selected cell if needed (do this outside of setNotebook)
    if (selectedCell >= newCellCount) {
      setSelectedCell(Math.max(0, newCellCount - 1));
    }

    setHasUnsavedChanges(true);
  };

  const moveCellUp = (index) => {
    if (index === 0) return;

    setNotebook(prevNotebook => {
      const newCells = [...prevNotebook.cells];
      [newCells[index - 1], newCells[index]] = [newCells[index], newCells[index - 1]];
      return { ...prevNotebook, cells: newCells };
    });

    setSelectedCell(index - 1);
    setHasUnsavedChanges(true);
  };

  const moveCellDown = (index) => {
    setNotebook(prevNotebook => {
      if (index === prevNotebook.cells.length - 1) return prevNotebook;

      const newCells = [...prevNotebook.cells];
      [newCells[index], newCells[index + 1]] = [newCells[index + 1], newCells[index]];
      return { ...prevNotebook, cells: newCells };
    });

    setSelectedCell(index + 1);
    setHasUnsavedChanges(true);
  };

  const changeCellType = (index, newType) => {
    setNotebook(prevNotebook => {
      const newCells = [...prevNotebook.cells];
      const cell = newCells[index];
      if (cell.cell_type === newType) return prevNotebook;

      cell.cell_type = newType;
      if (newType === "code") {
        cell.execution_count = null;
        cell.outputs = [];
      } else {
        delete cell.execution_count;
        delete cell.outputs;
      }
      return { ...prevNotebook, cells: newCells };
    });

    setHasUnsavedChanges(true);
  };

  const addCellAt = (index, type) => {
    const newCell = {
      cell_type: type,
      metadata: {},
      source: [],
    };

    if (type === "code") {
      newCell.execution_count = null;
      newCell.outputs = [];
    }

    setNotebook(prevNotebook => {
      const newCells = [...prevNotebook.cells];
      newCells.splice(index, 0, newCell);
      return { ...prevNotebook, cells: newCells };
    });

    setSelectedCell(index);
    setEditMode(true);
    setHasUnsavedChanges(true);
  };

  const executeCell = async (index, code, mode = 'shift-enter') => {
    const cell = notebook.cells[index];

    // For markdown cells, just render (exit edit mode) and navigate
    if (cell.cell_type === "markdown") {
      setEditMode(false);

      // Handle post-execution behavior based on mode
      if (mode === 'ctrl-enter') {
        // Ctrl+Enter: stay in current cell
      } else if (mode === 'alt-enter') {
        // Alt+Enter: insert new cell below and move to it
        const newCell = {
          cell_type: "code",
          metadata: {},
          source: [],
          execution_count: null,
          outputs: [],
        };
        const newCells = [...notebook.cells];
        newCells.splice(index + 1, 0, newCell);
        setNotebook({ ...notebook, cells: newCells });
        setSelectedCell(index + 1);
        setEditMode(true);
      } else {
        // Shift+Enter: move to next cell, or create new one if at end
        if (index < notebook.cells.length - 1) {
          setSelectedCell(index + 1);
          setEditMode(false);
        } else {
          // We're at the last cell, create a new code cell
          const newCell = {
            cell_type: "code",
            metadata: {},
            source: [],
            execution_count: null,
            outputs: [],
          };
          const newCells = [...notebook.cells];
          newCells.push(newCell);
          setNotebook({ ...notebook, cells: newCells });
          setSelectedCell(index + 1);
          setEditMode(true);
        }
      }
      return;
    }

    // For code cells, execute in kernel
    if (!engineStartedRef.current) {
      console.error("Engine not ready");
      return;
    }

    try {
      // Set kernel status to busy
      setEngineStatus('busy');
      setRunningCellIndex(index);

      // Start execution timer
      const startTime = Date.now();
      setCellExecutionStartTime(startTime);
      setCellExecutionElapsed(0);

      // Update elapsed time every 100ms
      if (executionTimerRef.current) {
        clearInterval(executionTimerRef.current);
      }
      executionTimerRef.current = setInterval(() => {
        setCellExecutionElapsed(Date.now() - startTime);
      }, 100);

      // Clear outputs before execution to show fresh results
      setNotebook(prevNotebook => {
        const newCells = [...prevNotebook.cells];
        const currentCell = newCells[index];
        currentCell.outputs = [];
        // Update the source to match what will be executed
        currentCell.source = code.split("\n").map((line, i, arr) =>
          i < arr.length - 1 ? line + "\n" : line
        );
        return { ...prevNotebook, cells: newCells };
      });

      // Clean up any existing output listener to prevent duplicates
      if (outputListenerRef.current) {
        outputListenerRef.current();
        outputListenerRef.current = null;
      }

      // Use streaming execution for real-time output
      // The backend will emit events with progressive outputs
      // Calculate event name (must match backend hash)
      const eventName = `cell-output-${hashString(workbookPath)}`;

      // Set up listener for streaming outputs
      const unlisten = await listen(eventName, (event) => {
        const output = event.payload;

        // Add output to cell progressively
        setNotebook(prevNotebook => {
          const newCells = [...prevNotebook.cells];
          const currentCell = newCells[index];

          // Check if this output was already added (React Strict Mode calls setState twice)
          // Compare with the last output to avoid duplicates
          const lastOutput = currentCell.outputs?.[currentCell.outputs.length - 1];
          const isDuplicate = lastOutput &&
            JSON.stringify(lastOutput) === JSON.stringify(output);

          if (isDuplicate) {
            return prevNotebook; // No change
          }

          // Append new output
          currentCell.outputs = [...(currentCell.outputs || []), output];

          // Update execution count if this is an execute_result
          if (output.output_type === 'execute_result' && output.execution_count) {
            currentCell.execution_count = output.execution_count;
          }

          return { ...prevNotebook, cells: newCells };
        });
        setHasUnsavedChanges(true);
      });

      // Store the unlisten function
      outputListenerRef.current = unlisten;

      // Start streaming execution
      await invoke("execute_cell_stream", {
        workbookPath: workbookPath,
        code: code,
      });

      // Clean up listener
      unlisten();
      outputListenerRef.current = null;

      // Ensure execution count is set even if no execute_result
      setNotebook(prevNotebook => {
        const newCells = [...prevNotebook.cells];
        const currentCell = newCells[index];
        if (!currentCell.execution_count) {
          currentCell.execution_count = (currentCell.execution_count || 0) + 1;
        }
        return { ...prevNotebook, cells: newCells };
      });
      setHasUnsavedChanges(true);

      // Clear any previous errors on successful execution
      setError(null);

      // Handle post-execution behavior based on mode
      if (mode === 'ctrl-enter') {
        // Ctrl+Enter: stay in current cell, don't move
        // Keep current selection and edit mode
      } else if (mode === 'alt-enter') {
        // Alt+Enter: insert new cell below and move to it
        const newCell = {
          cell_type: "code",
          metadata: {},
          source: [],
          execution_count: null,
          outputs: [],
        };
        setNotebook((prevNotebook) => {
          const cells = [...prevNotebook.cells];
          cells.splice(index + 1, 0, newCell);
          return { ...prevNotebook, cells };
        });
        setSelectedCell(index + 1);
        setEditMode(true);
      } else {
        // Shift+Enter: move to next cell, or create new one if at end
        // Keep edit mode ON so user can hit Shift+Enter repeatedly to run all cells
        if (index < notebook.cells.length - 1) {
          setSelectedCell(index + 1);
          setEditMode(true);
        } else {
          // We're at the last cell, create a new code cell
          const newCell = {
            cell_type: "code",
            metadata: {},
            source: [],
            execution_count: null,
            outputs: [],
          };
          setNotebook((prevNotebook) => {
            const cells = [...prevNotebook.cells];
            cells.push(newCell);
            return { ...prevNotebook, cells };
          });
          setSelectedCell(index + 1);
          setEditMode(true);
        }
      }

      // Set kernel status back to idle after execution
      setEngineStatus('idle');
      setRunningCellIndex(null);

      // Stop execution timer
      if (executionTimerRef.current) {
        clearInterval(executionTimerRef.current);
        executionTimerRef.current = null;
      }
      setCellExecutionStartTime(null);
    } catch (err) {
      console.error("Failed to execute cell:", err);
      const errorMsg = err.toString();

      // Stop execution timer on error
      if (executionTimerRef.current) {
        clearInterval(executionTimerRef.current);
        executionTimerRef.current = null;
      }
      setCellExecutionStartTime(null);

      // Check if kernel was not found - this indicates the kernel died or timed out
      if (errorMsg.includes("No kernel found") || errorMsg.includes("Engine server not initialized")) {
        setEngineStatus('error');
        engineStartedRef.current = false;
        setEngineReady(false);
        setError("Engine connection lost. Click 'Reconnect Engine' to restart.");
      } else {
        // Set kernel status back to idle even on error (unless kernel died)
        setEngineStatus('idle');
        setError(errorMsg);
      }
      setRunningCellIndex(null);
    }
  };

  const runAllCells = async () => {
    if (!engineStartedRef.current || isRunningAll) {
      console.log("Cannot run all cells - engine not ready or already running");
      return;
    }

    setIsRunningAll(true);
    setEngineStatus('busy');
    setError(null);

    try {
      // Execute all code cells in sequence
      for (let i = 0; i < notebook.cells.length; i++) {
        const cell = notebook.cells[i];
        if (cell.cell_type === "code") {
          setSelectedCell(i);
          setRunningCellIndex(i);

          // Get the source code before clearing output
          const code = cell.source.join("");

          // Clear the output before running to show it's executing
          setNotebook(prevNotebook => {
            const newCells = [...prevNotebook.cells];
            newCells[i].outputs = [];
            return { ...prevNotebook, cells: newCells };
          });

          // Execute the cell
          const result = await invoke("execute_cell", {
            workbookPath: workbookPath,
            code: code,
          });

          // Update the cell with outputs
          setNotebook(prevNotebook => {
            const newCells = [...prevNotebook.cells];
            newCells[i].outputs = result.outputs || [];

            const executionCount = result.outputs?.find(o => o.output_type === 'execute_result')?.execution_count;
            if (executionCount) {
              newCells[i].execution_count = executionCount;
            } else {
              newCells[i].execution_count = (newCells[i].execution_count || 0) + 1;
            }

            return { ...prevNotebook, cells: newCells };
          });
          setHasUnsavedChanges(true);
        }
      }
    } catch (err) {
      console.error("Failed to run all cells:", err);
      const errorMsg = err.toString();

      // Check if kernel was not found - this indicates the kernel died or timed out
      if (errorMsg.includes("No kernel found") || errorMsg.includes("Engine server not initialized")) {
        setEngineStatus('error');
        engineStartedRef.current = false;
        setEngineReady(false);
        setError("Engine connection lost. Click 'Reconnect Engine' to restart.");
      } else {
        setError(errorMsg);
      }
    } finally {
      setIsRunningAll(false);
      setRunningCellIndex(null);
      // Set kernel back to idle (unless it's in error state)
      if (engineStatus !== 'error') {
        setEngineStatus('idle');
      }
    }
  };

  const clearAllOutputs = () => {
    setNotebook(prevNotebook => {
      const newCells = prevNotebook.cells.map(cell => {
        if (cell.cell_type === "code") {
          return {
            ...cell,
            outputs: [],
            execution_count: null
          };
        }
        return cell;
      });
      return { ...prevNotebook, cells: newCells };
    });

    setHasUnsavedChanges(true);
  };

  const clearCellOutput = (index) => {
    setNotebook(prevNotebook => {
      const newCells = [...prevNotebook.cells];
      if (newCells[index].cell_type === "code") {
        newCells[index] = {
          ...newCells[index],
          outputs: [],
        };
        return { ...prevNotebook, cells: newCells };
      }
      return prevNotebook;
    });

    setHasUnsavedChanges(true);
  };

  const addCell = (type) => {
    const newCell = {
      cell_type: type,
      metadata: {},
      source: [],
    };

    if (type === "code") {
      newCell.execution_count = null;
      newCell.outputs = [];
    }

    setNotebook(prevNotebook => {
      const newCells = [...prevNotebook.cells, newCell];
      return { ...prevNotebook, cells: newCells };
    });

    // Select the new cell and enter edit mode
    setSelectedCell(notebook.cells.length);
    setEditMode(true);
    setHasUnsavedChanges(true);
  };

  const getWorkbookName = () => {
    return workbookPath.split("/").pop();
  };

  const handleClose = async () => {
    if (hasUnsavedChanges) {
      const shouldSave = await ask(
        "You have unsaved changes. Do you want to save before closing?",
        {
          title: "Unsaved Changes",
          kind: "warning",
          okLabel: "Save",
          cancelLabel: "Don't Save"
        }
      );

      if (shouldSave) {
        await saveWorkbook();
      }
    }

    // Close the notebook
    onClose();
  };

  if (loading) {
    return (
      <div className="notebook-viewer loading">
        <p>Loading notebook...</p>
      </div>
    );
  }

  if (!notebook) {
    return null;
  }

  const getEngineStatusText = () => {
    switch (engineStatus) {
      case 'starting':
        return '⏳ Starting...';
      case 'idle':
        return '● Idle';
      case 'busy':
        return '⚡ Busy';
      case 'restarting':
        return '🔄 Restarting...';
      case 'error':
        return '⚠ Error';
      default:
        return '';
    }
  };

  const isEngineReady = engineStatus === 'idle' || engineStatus === 'busy';

  return (
    <div className="notebook-viewer">
      <div className="notebook-header">
        <h2>
          {getWorkbookName()}
          {hasUnsavedChanges && <span className="unsaved-indicator"> •</span>}
        </h2>
        <div className="notebook-actions">
          <span className={`engine-status ${engineStatus}`} title={getEngineStatusText()}>
            {getEngineStatusText()}
          </span>
          {engineStatus === 'error' && (
            <button onClick={startEngine} title="Retry connecting to engine">
              Reconnect Engine
            </button>
          )}
          <span className="keyboard-hint">Shift+Enter to run</span>
          <button onClick={runAllCells} disabled={isRunningAll || !engineReady} title="Run all cells">
            {isRunningAll ? "Running..." : "Run All"}
          </button>
          <button onClick={interruptExecution} disabled={engineStatus !== 'busy'} title="Interrupt execution (stop running cell)">
            ⬛ Interrupt
          </button>
          <button onClick={clearAllOutputs} title="Clear all outputs">Clear All Outputs</button>
          <button onClick={restartEngine} disabled={!engineReady} title="Restart kernel">Restart Engine</button>
          <button onClick={() => addCell("markdown")} title="Add markdown cell">+ Markdown</button>
          <button onClick={() => addCell("code")} title="Add code cell">+ Code</button>
          <button onClick={saveWorkbook} className={hasUnsavedChanges ? "primary" : ""} title="Save notebook">
            Save {hasUnsavedChanges && "*"}
          </button>
        </div>
      </div>

      <div className="notebook-cells">
        {error && (
          <div className="notebook-error-banner">
            <span>{error}</span>
            <button onClick={() => setError(null)} title="Dismiss error">×</button>
          </div>
        )}
        {notebook.cells.length === 0 && (
          <div className="empty-notebook">
            <p>Empty notebook. Add a cell to get started.</p>
          </div>
        )}
        {notebook.cells.map((cell, index) => (
          <WorkbookCell
            key={index}
            cell={cell}
            index={index}
            onUpdate={updateCell}
            onDelete={deleteCell}
            onExecute={executeCell}
            onMoveUp={moveCellUp}
            onMoveDown={moveCellDown}
            onClearOutput={clearCellOutput}
            onInsertBelow={() => addCellAt(index + 1, "code")}
            isSelected={selectedCell === index}
            isEditMode={editMode && selectedCell === index}
            isRunning={runningCellIndex === index}
            executionElapsed={runningCellIndex === index ? cellExecutionElapsed : 0}
            onSelect={setSelectedCell}
            onEnterEditMode={() => {
              setSelectedCell(index);
              setEditMode(true);
            }}
            autosaveEnabled={autosaveEnabled}
          />
        ))}
      </div>
    </div>
  );
}
