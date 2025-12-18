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

function WorkbookCell({ cell, index, workbookPath, onUpdate, onDelete, onExecute, onMoveUp, onMoveDown, onClearOutput, isSelected, isEditMode, isRunning, executionElapsed, onSelect, onEnterEditMode, onInsertBelow, autosaveEnabled }) {
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
    const newValue = value || "";
    setContent(newValue);
    // Immediately sync content back to parent to ensure "Run All" uses current editor state
    onUpdate(index, newValue);
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
        className={`relative mb-4 px-4 py-3 rounded-lg transition-all ${
          isSelected ? 'bg-blue-50 ring-2 ring-blue-200' : 'hover:bg-gray-50'
        }`}
        onClick={() => onSelect(index)}
        onDoubleClick={() => {
          onSelect(index);
          onEnterEditMode();
        }}
      >
        {isSelected && (
          <div className="absolute top-2 right-2 flex gap-1 z-10">
            <button
              onClick={(e) => { e.stopPropagation(); handleExecute(); }}
              className="px-2 py-1 text-xs bg-white hover:bg-gray-100 border border-gray-300 rounded shadow-sm transition-colors"
              title="Run cell (render)"
            >
              ▶
            </button>
            <button
              onClick={(e) => { e.stopPropagation(); onMoveUp(index); }}
              className="px-2 py-1 text-xs bg-white hover:bg-gray-100 border border-gray-300 rounded shadow-sm transition-colors"
              title="Move up"
            >
              ↑
            </button>
            <button
              onClick={(e) => { e.stopPropagation(); onMoveDown(index); }}
              className="px-2 py-1 text-xs bg-white hover:bg-gray-100 border border-gray-300 rounded shadow-sm transition-colors"
              title="Move down"
            >
              ↓
            </button>
            <button
              onClick={(e) => { e.stopPropagation(); onDelete(index); }}
              className="px-2 py-1 text-xs bg-white hover:bg-gray-100 border border-gray-300 rounded shadow-sm transition-colors"
              title="Delete"
            >
              🗑
            </button>
          </div>
        )}
        <div className="w-full">
          {isEditMode ? (
            <textarea
              value={content}
              onChange={(e) => handleEditorChange(e.target.value)}
              onKeyDown={handleKeyDown}
              onBlur={handleBlur}
              className="w-full p-3 border-none resize-y font-sans text-sm leading-relaxed focus:outline-none bg-transparent"
              placeholder="Enter markdown..."
              rows={Math.max(3, content.split("\n").length)}
              autoFocus
            />
          ) : (
            <div className="markdown-content px-3 py-2">
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
                <div className="text-gray-400 italic px-3 py-2">Double-click to edit markdown</div>
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
        className={`relative flex gap-3 mb-4 ${isSelected ? 'bg-blue-50/30' : ''}`}
        onClick={() => onSelect(index)}
      >
        {isSelected && (
          <div className="absolute -top-1 right-2 flex gap-1 z-10">
            <button
              onClick={(e) => { e.stopPropagation(); handleExecute(); }}
              className="px-2 py-1 text-xs bg-white hover:bg-gray-100 border border-gray-300 rounded shadow-sm transition-colors"
              title="Run cell"
            >
              ▶
            </button>
            {hasOutput && (
              <button
                onClick={(e) => { e.stopPropagation(); onClearOutput(index); }}
                className="px-2 py-1 text-xs bg-white hover:bg-gray-100 border border-gray-300 rounded shadow-sm transition-colors"
                title="Clear output"
              >
                🗙
              </button>
            )}
            <button
              onClick={(e) => { e.stopPropagation(); onMoveUp(index); }}
              className="px-2 py-1 text-xs bg-white hover:bg-gray-100 border border-gray-300 rounded shadow-sm transition-colors"
              title="Move up"
            >
              ↑
            </button>
            <button
              onClick={(e) => { e.stopPropagation(); onMoveDown(index); }}
              className="px-2 py-1 text-xs bg-white hover:bg-gray-100 border border-gray-300 rounded shadow-sm transition-colors"
              title="Move down"
            >
              ↓
            </button>
            <button
              onClick={(e) => { e.stopPropagation(); onDelete(index); }}
              className="px-2 py-1 text-xs bg-white hover:bg-gray-100 border border-gray-300 rounded shadow-sm transition-colors"
              title="Delete"
            >
              🗑
            </button>
          </div>
        )}
        <div className={`font-mono text-xs text-gray-500 min-w-[60px] pt-2 pr-2 text-right flex-shrink-0 ${isRunning ? 'text-blue-600 font-semibold' : ''}`}>
          <div>[{cell.execution_count || " "}]</div>
          {isRunning && executionElapsed > 0 && (
            <div className="text-[10px] text-blue-600 font-medium mt-0.5" title="Execution time">
              {(executionElapsed / 1000).toFixed(1)}s
            </div>
          )}
        </div>
        <div className={`flex-1 flex flex-col ${isSelected ? 'border-l-2 border-blue-500 pl-3' : 'border-l-2 border-transparent pl-3'} rounded transition-all`}>
          <div className="cell-input">
            <Editor
              height={`${Math.max(100, content.split("\n").length * 19)}px`}
              defaultLanguage="python"
              value={content}
              onChange={handleEditorChange}
              onMount={(editor, monaco) => {
                editorRef.current = editor;

                // Focus editor if cell is in edit mode when it mounts
                if (isEditMode && isSelected) {
                  setTimeout(() => {
                    editor.focus();
                  }, 50);
                }

                // Register completion provider for Python
                const disposable = monaco.languages.registerCompletionItemProvider('python', {
                  triggerCharacters: ['.', ' '],
                  provideCompletionItems: async (model, position) => {
                    try {
                      const code = model.getValue();
                      const offset = model.getOffsetAt(position);

                      // Call the Tauri command to get completions
                      const result = await invoke("complete_code", {
                        workbookPath: workbookPath,
                        code: code,
                        cursorPos: offset,
                      });

                      if (!result || !result.matches || result.matches.length === 0) {
                        return { suggestions: [] };
                      }

                      // Calculate the range based on cursor positions from kernel
                      const startPos = model.getPositionAt(result.cursor_start);
                      const endPos = model.getPositionAt(result.cursor_end);

                      // Convert matches to Monaco completion items
                      const suggestions = result.matches.map((match) => ({
                        label: match.text,
                        kind: monaco.languages.CompletionItemKind.Function,
                        insertText: match.text,
                        detail: 'Python',
                        range: {
                          startLineNumber: startPos.lineNumber,
                          startColumn: startPos.column,
                          endLineNumber: endPos.lineNumber,
                          endColumn: endPos.column,
                        },
                      }));

                      return { suggestions };
                    } catch (error) {
                      console.error("Failed to get completions:", error);
                      return { suggestions: [] };
                    }
                  },
                });

                // Clean up provider on unmount
                editor.onDidDispose(() => {
                  disposable.dispose();
                });

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
                // Autocomplete settings
                suggestOnTriggerCharacters: true,
                quickSuggestions: {
                  other: true,
                  comments: false,
                  strings: false,
                },
                acceptSuggestionOnEnter: "on",
                tabCompletion: "on",
                wordBasedSuggestions: false, // Disable word-based, use only our provider
                suggest: {
                  showMethods: true,
                  showFunctions: true,
                  showConstructors: true,
                  showFields: true,
                  showVariables: true,
                  showClasses: true,
                  showModules: true,
                  showProperties: true,
                  showKeywords: true,
                  showSnippets: true,
                  insertMode: 'replace',
                },
              }}
            />
          </div>
          {hasOutput && (
            <div className="mt-2 border-t border-gray-200 bg-gray-50 rounded-b">
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
      <div className={className}>
        <div className="cell-output-content p-3 max-h-[300px] overflow-auto">
          {isTruncatedByBackend && (
            <div className="px-3 py-2 mb-2 bg-amber-50 border border-amber-200 rounded text-amber-800 text-xs font-medium">
              ⚠ Output was truncated to save memory (max 1000 lines or 100KB per cell)
            </div>
          )}
          <pre className="m-0 whitespace-pre-wrap break-words">{displayText}</pre>
        </div>
        {truncated && (
          <button
            className="block mx-3 my-2 px-3 py-1 text-xs font-medium bg-white hover:bg-gray-50 border border-gray-300 rounded text-blue-600 transition-colors"
            onClick={() => setExpanded(!expanded)}
          >
            Show more ({totalLines - MAX_LINES} more lines)
          </button>
        )}
        {expanded && (
          <button
            className="block mx-3 my-2 px-3 py-1 text-xs font-medium bg-white hover:bg-gray-50 border border-gray-300 rounded text-blue-600 transition-colors"
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
        <div>
          <div className="cell-output-content p-3">
            <img
              src={`data:image/png;base64,${data["image/png"]}`}
              alt="Output"
              className="max-w-full h-auto mx-auto rounded"
            />
          </div>
        </div>
      );
    }

    // Images (JPEG)
    if (data["image/jpeg"]) {
      return (
        <div>
          <div className="cell-output-content p-3">
            <img
              src={`data:image/jpeg;base64,${data["image/jpeg"]}`}
              alt="Output"
              className="max-w-full h-auto mx-auto rounded"
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
        <div>
          <div className="cell-output-content p-3">
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
        <div>
          <div className="cell-output-content p-3">
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
        <div>
          <div className="cell-output-content p-3 max-h-[300px] overflow-auto">
            {isTruncatedByBackend && (
              <div className="px-3 py-2 mb-2 bg-amber-50 border border-amber-200 rounded text-amber-800 text-xs font-medium">
                ⚠ Output was truncated to save memory (max 1000 lines or 100KB per cell)
              </div>
            )}
            <pre className="m-0 whitespace-pre-wrap break-words">{displayText}</pre>
          </div>
          {truncated && (
            <button
              className="block mx-3 my-2 px-3 py-1 text-xs font-medium bg-white hover:bg-gray-50 border border-gray-300 rounded text-blue-600 transition-colors"
              onClick={() => setExpanded(!expanded)}
            >
              Show more ({totalLines - MAX_LINES} more lines)
            </button>
          )}
          {expanded && (
            <button
              className="block mx-3 my-2 px-3 py-1 text-xs font-medium bg-white hover:bg-gray-50 border border-gray-300 rounded text-blue-600 transition-colors"
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
      <div>
        <div className="cell-output-content p-3">
          <pre className="m-0 whitespace-pre-wrap break-words">{JSON.stringify(data, null, 2)}</pre>
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
      <div className="bg-red-50">
        <div className="cell-output-content p-3 max-h-[300px] overflow-auto text-red-700">
          {isTruncatedByBackend && (
            <div className="px-3 py-2 mb-2 bg-amber-50 border border-amber-200 rounded text-amber-800 text-xs font-medium">
              ⚠ Output was truncated to save memory (max 1000 lines or 100KB per cell)
            </div>
          )}
          <pre className="m-0 whitespace-pre-wrap break-words">{displayText}</pre>
        </div>
        {truncated && (
          <button
            className="block mx-3 my-2 px-3 py-1 text-xs font-medium bg-white hover:bg-gray-50 border border-gray-300 rounded text-blue-600 transition-colors"
            onClick={() => setExpanded(!expanded)}
          >
            Show more ({totalLines - MAX_LINES} more lines)
          </button>
        )}
        {expanded && (
          <button
            className="block mx-3 my-2 px-3 py-1 text-xs font-medium bg-white hover:bg-gray-50 border border-gray-300 rounded text-blue-600 transition-colors"
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

export function WorkbookViewer({ workbookPath, projectRoot, autosaveEnabled = true, onClose, onUnsavedChangesUpdate }) {
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

  // Notify parent when unsaved changes state changes
  useEffect(() => {
    if (onUnsavedChangesUpdate) {
      onUnsavedChangesUpdate(hasUnsavedChanges);
    }
  }, [hasUnsavedChanges]);

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
    } catch (err) {
      console.error("Failed to load notebook:", err);
      setError(err.toString());
    } finally {
      setLoading(false);
    }
  };

  const saveWorkbook = async () => {
    if (!notebook) return;

    try {
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

    // Don't execute empty code cells
    if (cell.cell_type === "code" && !code.trim()) {
      // Just handle navigation for empty cells without executing
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
        // Shift+Enter: move to next cell
        if (index < notebook.cells.length - 1) {
          setSelectedCell(index + 1);
          setEditMode(true);
        } else {
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
      const result = await invoke("execute_cell_stream", {
        workbookPath: workbookPath,
        code: code,
      });

      // Clean up listener
      unlisten();
      outputListenerRef.current = null;

      // Set execution count from the result
      if (result && result.execution_count !== null && result.execution_count !== undefined) {
        setNotebook(prevNotebook => {
          const newCells = [...prevNotebook.cells];
          newCells[index].execution_count = result.execution_count;
          return { ...prevNotebook, cells: newCells };
        });
        setHasUnsavedChanges(true);
      }

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
          // Get the source code before clearing output
          const code = cell.source.join("");

          // Skip empty cells
          if (!code.trim()) {
            continue;
          }

          setSelectedCell(i);
          setRunningCellIndex(i);

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

          // Update the cell with outputs and execution count
          setNotebook(prevNotebook => {
            const newCells = [...prevNotebook.cells];
            newCells[i].outputs = result.outputs || [];

            // Set execution_count from the result
            if (result.execution_count !== null && result.execution_count !== undefined) {
              newCells[i].execution_count = result.execution_count;
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
      <div className="flex flex-col items-center justify-center h-full bg-white">
        <p className="text-gray-500">Loading notebook...</p>
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
    <div className="flex flex-col h-full bg-white">
      <div className="px-6 py-4 border-b border-gray-200 bg-white">
        <div className="flex items-start justify-between gap-4 mb-3">
          <h2 className="text-base font-semibold text-gray-900 flex items-center gap-2">
            {getWorkbookName()}
            {hasUnsavedChanges && <span className="text-amber-500 text-lg">•</span>}
          </h2>
          <div className="flex items-center gap-2">
            <span className={`text-xs px-2 py-1 rounded-md font-medium ${
              engineStatus === 'starting' ? 'bg-amber-50 text-amber-700' :
              engineStatus === 'idle' ? 'bg-emerald-50 text-emerald-700' :
              engineStatus === 'busy' ? 'bg-blue-50 text-blue-700 animate-pulse-subtle' :
              engineStatus === 'restarting' ? 'bg-amber-50 text-amber-700' :
              'bg-red-50 text-red-700'
            }`} title={getEngineStatusText()}>
              {getEngineStatusText()}
            </span>
            {hasUnsavedChanges && (
              <button
                onClick={saveWorkbook}
                className="px-3 py-1.5 text-sm font-medium text-white bg-blue-600 hover:bg-blue-700 rounded-md transition-colors shadow-sm"
                title="Save notebook"
              >
                Save
              </button>
            )}
          </div>
        </div>
        <div className="flex items-center gap-2 flex-wrap">
          <span className="text-xs px-2 py-1 bg-gray-100 text-gray-600 rounded font-mono">Shift+Enter to run</span>
          <div className="flex-1" />
          <div className="flex items-center gap-1.5">
            {engineStatus === 'error' && (
              <button
                onClick={startEngine}
                className="px-2.5 py-1.5 text-xs font-medium text-gray-700 bg-white hover:bg-gray-50 border border-gray-300 rounded-md transition-all"
                title="Retry connecting to engine"
              >
                Reconnect Engine
              </button>
            )}
            <button
              onClick={runAllCells}
              disabled={isRunningAll || !isEngineReady}
              className="px-2.5 py-1.5 text-xs font-medium text-gray-700 bg-white hover:bg-gray-50 border border-gray-300 rounded-md transition-all disabled:opacity-50 disabled:cursor-not-allowed"
              title="Run all cells"
            >
              {isRunningAll ? "Running..." : "Run All"}
            </button>
            <button
              onClick={interruptExecution}
              disabled={engineStatus !== 'busy'}
              className="px-2.5 py-1.5 text-xs font-medium text-gray-700 bg-white hover:bg-gray-50 border border-gray-300 rounded-md transition-all disabled:opacity-50 disabled:cursor-not-allowed"
              title="Interrupt execution"
            >
              ⬛ Interrupt
            </button>
            <button
              onClick={clearAllOutputs}
              className="px-2.5 py-1.5 text-xs font-medium text-gray-700 bg-white hover:bg-gray-50 border border-gray-300 rounded-md transition-all"
              title="Clear all outputs"
            >
              Clear All
            </button>
            <button
              onClick={restartEngine}
              disabled={!isEngineReady}
              className="px-2.5 py-1.5 text-xs font-medium text-gray-700 bg-white hover:bg-gray-50 border border-gray-300 rounded-md transition-all disabled:opacity-50 disabled:cursor-not-allowed"
              title="Restart kernel"
            >
              Restart
            </button>
            <div className="w-px h-4 bg-gray-300"></div>
            <button
              onClick={() => addCell("markdown")}
              className="px-2.5 py-1.5 text-xs font-medium text-gray-700 bg-white hover:bg-gray-50 border border-gray-300 rounded-md transition-all"
              title="Add markdown cell"
            >
              + Markdown
            </button>
            <button
              onClick={() => addCell("code")}
              className="px-2.5 py-1.5 text-xs font-medium text-gray-700 bg-white hover:bg-gray-50 border border-gray-300 rounded-md transition-all"
              title="Add code cell"
            >
              + Code
            </button>
          </div>
        </div>
      </div>

      <div className="flex-1 overflow-y-auto px-6 py-6 custom-scrollbar">
        {error && (
          <div className="bg-red-50 border border-red-200 rounded-lg px-4 py-3 mb-4 flex items-center justify-between text-red-800 text-sm">
            <span>{error}</span>
            <button
              onClick={() => setError(null)}
              className="text-red-600 hover:bg-red-100 rounded px-2 py-1 transition-colors text-lg font-bold"
              title="Dismiss error"
            >
              ×
            </button>
          </div>
        )}
        {notebook.cells.length === 0 && (
          <div className="text-center py-16 text-gray-400">
            <p>Empty notebook. Add a cell to get started.</p>
          </div>
        )}
        {notebook.cells.map((cell, index) => (
          <WorkbookCell
            key={index}
            cell={cell}
            index={index}
            workbookPath={workbookPath}
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
