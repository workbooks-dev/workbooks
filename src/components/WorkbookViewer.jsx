import { useState, useEffect, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import { ask } from "@tauri-apps/plugin-dialog";
import { listen } from "@tauri-apps/api/event";
import { convertFileSrc } from "@tauri-apps/api/core";
import Editor from "@monaco-editor/react";
import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";
import remarkMath from "remark-math";
import rehypeKatex from "rehype-katex";
import rehypeRaw from "rehype-raw";
import { Prism as SyntaxHighlighter } from "react-syntax-highlighter";
import { vscDarkPlus } from "react-syntax-highlighter/dist/esm/styles/prism";
import { SecretsWarningModal } from "./SecretsWarningModal";

// ============================================
// STYLING CONSTANTS - DO NOT MODIFY
// All className strings are defined here
// Any style changes will be clearly visible in git diff
// ============================================
const STYLES = {
  // Cell container styles
  cell: {
    markdownContainer: "relative mb-4 px-2 py-2 pl-3 border-l-2 transition-all",
    markdownSelected: "bg-gray-50/30 border-l-blue-500",
    markdownUnselected: "border-l-transparent",
    codeContainer: "relative mb-4 pl-3 border-l-2 transition-all",
    codeSelected: "bg-gray-50/30 border-l-blue-500",
    codeUnselected: "border-l-transparent",
  },

  // Cell action buttons
  button: {
    actionBar: "absolute top-2 right-2 flex gap-1 z-10",
    actionBarCode: "absolute -top-2 right-0 flex gap-1 z-10",
    action: "px-2 py-1 text-xs bg-white hover:bg-gray-100 border border-gray-300 rounded shadow-sm transition-colors",
  },

  // Markdown cell
  markdown: {
    container: "w-full",
    textarea: "w-full p-3 border-none resize-y font-sans text-sm leading-relaxed focus:outline-none bg-transparent",
    content: "markdown-content prose prose-sm max-w-none px-3 py-2",
    placeholder: "text-gray-400 italic px-3 py-2",
  },

  // Code cell execution area
  execution: {
    container: "flex gap-2",
    countContainer: "font-mono text-xs min-w-[20px] pt-2 pr-1 text-right flex-shrink-0",
    countError: "text-red-600 font-semibold",
    countRunning: "text-blue-600 font-semibold",
    countNormal: "text-gray-500",
    countText: "font-medium",
    errorIndicator: "text-red-600 mr-0.5",
    timer: "text-[10px] text-blue-600 font-medium mt-0.5",
    metadata: "text-[10px] text-gray-500 mt-0.5",
  },

  // Code editor
  editor: {
    container: "flex-1 flex flex-col min-w-0",
    input: "cell-input rounded-lg bg-white border transition-all px-2",
    inputSelected: "border-blue-400 shadow-sm",
    inputUnselected: "border-gray-300",
  },

  // Output area
  output: {
    container: "mt-2",
    secretsWarning: "cell-output-content p-3 bg-yellow-50 border-t border-yellow-200",
    secretsText: "flex items-center gap-2 text-yellow-800 text-sm font-medium",
    secretsIcon: "w-4 h-4",

    // Stream output
    streamContainer: "rounded-lg border border-red-200 bg-red-50/30",
    streamNormal: "bg-gray-50",
    streamContent: "p-2 max-h-[300px] overflow-auto",
    streamPre: "m-0 whitespace-pre-wrap break-words font-mono text-xs",
    streamStderr: "text-red-700",
    streamStdout: "text-gray-900",
    truncateWarning: "px-2 py-1 mb-2 bg-amber-50 border border-amber-200 rounded text-amber-800 text-xs",
    expandButton: "block mx-2 mb-2 px-2 py-1 text-xs font-medium bg-white hover:bg-gray-50 border border-gray-300 rounded text-blue-600 transition-colors",

    // Rich output
    imageContainer: "py-3",
    image: "max-w-full h-auto rounded-lg",
    svgContainer: "py-3",
    dataframeWrapper: "rounded-lg bg-gray-50 overflow-hidden",
    dataframe: "dataframe-output",
    plainWrapper: "bg-gray-50",
    plainContent: "p-2 max-h-[300px] overflow-auto",
    plainPre: "m-0 whitespace-pre-wrap break-words font-mono text-xs text-gray-900",
    rawData: "rounded-lg bg-gray-50 px-5 py-3",

    // Error output
    errorContainer: "rounded-lg border border-red-200 bg-red-50/30",
    errorContent: "p-2 max-h-[300px] overflow-auto",
    errorPre: "m-0 whitespace-pre-wrap break-words font-mono text-xs text-red-700",
  },

  // Main viewer
  viewer: {
    container: "flex flex-col h-full bg-gray-50",
    header: "px-6 py-4 border-b border-gray-200 bg-white",
    headerTop: "flex items-start justify-between gap-4 mb-3",
    title: "text-base font-semibold text-gray-900 flex items-center gap-2",
    unsavedDot: "text-amber-500 text-lg",
    headerControls: "flex items-center gap-2",
    statusBadge: "text-xs px-2 py-1 rounded-md font-medium",
    statusStarting: "bg-amber-50 text-amber-700",
    statusIdle: "bg-emerald-50 text-emerald-700",
    statusBusy: "bg-blue-50 text-blue-700 animate-pulse-subtle",
    statusRestarting: "bg-amber-50 text-amber-700",
    statusError: "bg-red-50 text-red-700",
    saveButton: "px-3 py-1.5 text-sm font-medium text-white rounded-md transition-colors shadow-sm",
    saveNormal: "bg-blue-600 hover:bg-blue-700",
    saveWarning: "bg-amber-600 hover:bg-amber-700",

    toolbar: "flex items-center gap-3 flex-wrap",
    hint: "text-xs px-2 py-1 bg-gray-100 text-gray-600 rounded-md font-mono",
    spacer: "flex-1",
    buttonGroup: "flex items-center gap-2",
    separator: "w-px h-5 bg-gray-300",

    button: "px-3 py-1.5 text-xs font-medium text-gray-700 bg-white hover:bg-gray-50 border border-gray-300 rounded-md transition-all disabled:opacity-50 disabled:cursor-not-allowed shadow-sm",
    buttonPrimary: "px-3 py-1.5 text-xs font-medium text-white bg-blue-600 hover:bg-blue-700 rounded-md transition-colors shadow-sm",

    content: "flex-1 overflow-y-auto px-4 py-4 custom-scrollbar",
    error: "bg-red-50 border border-red-200 rounded-lg px-4 py-3 mb-4 flex items-center justify-between text-red-800 text-sm",
    errorClose: "text-red-600 hover:bg-red-100 rounded px-2 py-1 transition-colors text-lg font-bold",
    emptyState: "text-center py-16 text-gray-400",
  },
};

// Hash function to match Rust implementation in lib.rs
function hashString(s) {
  let hash = 0;
  for (let i = 0; i < s.length; i++) {
    hash = ((hash << 5) - hash) + s.charCodeAt(i);
    hash = hash & hash; // Convert to 32-bit integer
  }
  return Math.abs(hash);
}

// Generate a unique ID for cells
function generateCellId() {
  return `cell-${Date.now()}-${Math.random().toString(36).substr(2, 9)}`;
}

// Ensure all cells have IDs
function ensureCellIds(cells) {
  return cells.map(cell => ({
    ...cell,
    metadata: {
      ...cell.metadata,
      cell_id: cell.metadata?.cell_id || generateCellId()
    }
  }));
}

function WorkbookCell({ cell, index, workbookPath, onUpdate, onDelete, onExecute, onMoveUp, onMoveDown, onClearOutput, isSelected, isEditMode, isRunning, executionElapsed, onSelect, onEnterEditMode, onInsertBelow, autosaveEnabled, swappingCells }) {
  // Initialize content from cell source ONCE on mount - don't sync after that
  const [content, setContent] = useState(cell.source.join(""));
  const editorRef = useRef(null);

  // Check if this cell is being swapped
  const isSwapping = swappingCells && (
    cell.metadata?.cell_id === swappingCells.cellId1 ||
    cell.metadata?.cell_id === swappingCells.cellId2
  );

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
        className={`${STYLES.cell.markdownContainer} ${
          isSelected ? STYLES.cell.markdownSelected : STYLES.cell.markdownUnselected
        }`}
        onClick={() => onSelect(index)}
        onDoubleClick={() => {
          onSelect(index);
          onEnterEditMode();
        }}
      >
        {isSelected && (
          <div className={STYLES.button.actionBar}>
            <button
              onClick={(e) => { e.stopPropagation(); handleExecute(); }}
              className={STYLES.button.action}
              title="Run cell (render)"
            >
              ▶
            </button>
            <button
              onClick={(e) => { e.stopPropagation(); onMoveUp(index); }}
              className={STYLES.button.action}
              title="Move up"
            >
              ↑
            </button>
            <button
              onClick={(e) => { e.stopPropagation(); onMoveDown(index); }}
              className={STYLES.button.action}
              title="Move down"
            >
              ↓
            </button>
            <button
              onClick={(e) => { e.stopPropagation(); onDelete(index); }}
              className={STYLES.button.action}
              title="Delete"
            >
              🗑
            </button>
          </div>
        )}
        <div className={STYLES.markdown.container}>
          {isEditMode ? (
            <textarea
              value={content}
              onChange={(e) => handleEditorChange(e.target.value)}
              onKeyDown={handleKeyDown}
              onBlur={handleBlur}
              className={STYLES.markdown.textarea}
              placeholder="Enter markdown..."
              rows={Math.max(3, content.split("\n").length)}
              autoFocus
            />
          ) : (
            <div className={STYLES.markdown.content}>
              {content ? (
                <ReactMarkdown
                  remarkPlugins={[remarkGfm, remarkMath]}
                  rehypePlugins={[rehypeKatex, rehypeRaw]}
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
                    },
                    img({ node, src, alt, ...props }) {
                      // Handle local file paths
                      let imgSrc = src;

                      // If it's a relative path or absolute local path, convert it
                      if (src && !src.startsWith('http://') && !src.startsWith('https://') && !src.startsWith('data:')) {
                        // Replace $TETHER_PROJECT_FOLDER with actual project root
                        // This supports both $TETHER_PROJECT_FOLDER and ${TETHER_PROJECT_FOLDER}
                        const projectRootPath = workbookPath.substring(0, workbookPath.lastIndexOf('/notebooks'));
                        imgSrc = imgSrc.replace(/\$\{?TETHER_PROJECT_FOLDER\}?/g, projectRootPath);

                        // Check if it's a relative path (after variable replacement)
                        if (!imgSrc.startsWith('/')) {
                          // Relative to workbook's directory (notebooks folder)
                          const workbookDir = workbookPath.substring(0, workbookPath.lastIndexOf('/'));
                          imgSrc = `${workbookDir}/${imgSrc}`;
                        }

                        // Convert to Tauri asset protocol
                        imgSrc = convertFileSrc(imgSrc);
                      }

                      return (
                        <img
                          src={imgSrc}
                          alt={alt || ''}
                          className="max-w-full h-auto rounded-lg my-2"
                          onError={(e) => {
                            e.target.onerror = null;
                            e.target.src = 'data:image/svg+xml,<svg xmlns="http://www.w3.org/2000/svg" width="100" height="100"><text x="10" y="50">Image not found</text></svg>';
                          }}
                          {...props}
                        />
                      );
                    },
                    a({ node, href, children, ...props }) {
                      // Handle local file links
                      const isLocalFile = href && !href.startsWith('http://') && !href.startsWith('https://') && !href.startsWith('#');

                      if (isLocalFile) {
                        return (
                          <a
                            href="#"
                            onClick={(e) => {
                              e.preventDefault();
                              // TODO: Open local file in appropriate viewer
                              console.log('Open local file:', href);
                            }}
                            className="text-blue-600 hover:text-blue-800 underline cursor-pointer"
                            title={`Local file: ${href}`}
                            {...props}
                          >
                            {children}
                          </a>
                        );
                      }

                      return (
                        <a
                          href={href}
                          target="_blank"
                          rel="noopener noreferrer"
                          className="text-blue-600 hover:text-blue-800 underline"
                          {...props}
                        >
                          {children}
                        </a>
                      );
                    },
                    table({ node, children, ...props }) {
                      return (
                        <div className="overflow-x-auto my-4">
                          <table className="min-w-full divide-y divide-gray-300 border border-gray-300" {...props}>
                            {children}
                          </table>
                        </div>
                      );
                    },
                    thead({ node, children, ...props }) {
                      return <thead className="bg-gray-100" {...props}>{children}</thead>;
                    },
                    th({ node, children, ...props }) {
                      return (
                        <th className="px-3 py-2 text-left text-xs font-semibold text-gray-900 border-r border-gray-300" {...props}>
                          {children}
                        </th>
                      );
                    },
                    td({ node, children, ...props }) {
                      return (
                        <td className="px-3 py-2 text-sm text-gray-700 border-r border-gray-300" {...props}>
                          {children}
                        </td>
                      );
                    },
                    tr({ node, children, ...props }) {
                      return <tr className="border-b border-gray-300 hover:bg-gray-50" {...props}>{children}</tr>;
                    }
                  }}
                >
                  {content}
                </ReactMarkdown>
              ) : (
                <div className={STYLES.markdown.placeholder}>Double-click to edit markdown</div>
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
    const hasError = outputs.some(output => output.output_type === "error");

    return (
      <div
        className={`${STYLES.cell.codeContainer} ${
          isSelected ? STYLES.cell.codeSelected : STYLES.cell.codeUnselected
        }`}
        onClick={() => onSelect(index)}
      >
        {isSelected && (
          <div className={STYLES.button.actionBarCode}>
            <button
              onClick={(e) => { e.stopPropagation(); handleExecute(); }}
              className={STYLES.button.action}
              title="Run cell"
            >
              ▶
            </button>
            {hasOutput && (
              <button
                onClick={(e) => { e.stopPropagation(); onClearOutput(index); }}
                className={STYLES.button.action}
                title="Clear output"
              >
                🗙
              </button>
            )}
            <button
              onClick={(e) => { e.stopPropagation(); onMoveUp(index); }}
              className={STYLES.button.action}
              title="Move up"
            >
              ↑
            </button>
            <button
              onClick={(e) => { e.stopPropagation(); onMoveDown(index); }}
              className={STYLES.button.action}
              title="Move down"
            >
              ↓
            </button>
            <button
              onClick={(e) => { e.stopPropagation(); onDelete(index); }}
              className={STYLES.button.action}
              title="Delete"
            >
              🗑
            </button>
          </div>
        )}
        <div className={STYLES.execution.container}>
          <div className={`${STYLES.execution.countContainer} ${
            hasError ? STYLES.execution.countError :
            isRunning ? STYLES.execution.countRunning : STYLES.execution.countNormal
          }`}>
            <div className={STYLES.execution.countText}>
              {hasError && <span className={STYLES.execution.errorIndicator} title="Cell execution failed">✗</span>}
              [{cell.execution_count || " "}]
            </div>
            {isRunning && executionElapsed > 0 && (
              <div className={STYLES.execution.timer} title="Execution time">
                {(executionElapsed / 1000).toFixed(1)}s
              </div>
            )}
            {!isRunning && cell.metadata?.tether?.duration_ms && (
              <div className={STYLES.execution.metadata} title={`Last run: ${cell.metadata.tether.last_run ? new Date(cell.metadata.tether.last_run).toLocaleString() : 'Unknown'}`}>
                {(cell.metadata.tether.duration_ms / 1000).toFixed(2)}s
              </div>
            )}
          </div>
          <div className={STYLES.editor.container}>
            <div className={`${STYLES.editor.input} ${
              isSelected ? STYLES.editor.inputSelected : STYLES.editor.inputUnselected
            }`}>
            {!isSwapping && (
              <Editor
                key={cell.metadata?.cell_id}
                height={`${Math.max(60, content.split("\n").length * 19 + 24)}px`}
                defaultLanguage="python"
                value={content}
                onChange={handleEditorChange}
                loading=""
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
                lineHeight: 19,
                lineNumbers: "off",
                glyphMargin: false,
                folding: false,
                lineDecorationsWidth: 0,
                scrollBeyondLastLine: false,
                automaticLayout: true,
                wordWrap: "on",
                padding: { top: 12, bottom: 12, left: 20, right: 20 },
                scrollbar: {
                  vertical: "hidden",
                  horizontal: "hidden",
                  useShadows: false,
                },
                renderLineHighlight: "none",
                overviewRulerBorder: false,
                hideCursorInOverviewRuler: true,
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
            )}
            {isSwapping && (
              <div style={{
                height: `${Math.max(60, content.split("\n").length * 19 + 24)}px`,
                backgroundColor: '#f9fafb'
              }} />
            )}
            </div>
            {hasOutput && (
              <div className={STYLES.output.container}>
                {outputs.map((output, idx) => (
                  <CellOutput key={idx} output={output} />
                ))}
              </div>
            )}
          </div>
        </div>
      </div>
    );
  }

  return null;
}

function CellOutput({ output }) {
  const [expanded, setExpanded] = useState(false);
  const [zoomedImage, setZoomedImage] = useState(null);
  const MAX_LINES = 20; // Show first 20 lines by default

  // Check if output was truncated by the backend
  const isTruncatedByBackend = output.metadata && output.metadata.truncated === true;

  // Check if output contains secrets
  const containsSecrets = output.metadata && output.metadata.contains_secrets === true;

  // If output contains secrets, show redacted message
  if (containsSecrets) {
    return (
      <div className={STYLES.output.secretsWarning}>
        <div className={STYLES.output.secretsText}>
          <svg className={STYLES.output.secretsIcon} fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M12 15v2m-6 4h12a2 2 0 002-2v-6a2 2 0 00-2-2H6a2 2 0 00-2 2v6a2 2 0 002 2zm10-10V7a4 4 0 00-8 0v4h8z" />
          </svg>
          <span>[secret hidden here]</span>
        </div>
      </div>
    );
  }

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
    const isStderr = output.name === "stderr";
    const cleanText = stripAnsi(text);

    const { text: displayText, truncated, totalLines } = expanded
      ? { text: cleanText, truncated: false, totalLines: 0 }
      : truncateText(cleanText, MAX_LINES);

    return (
      <div className={isStderr ? STYLES.output.streamContainer : STYLES.output.streamNormal}>
        <div className={STYLES.output.streamContent}>
          {isTruncatedByBackend && (
            <div className={STYLES.output.truncateWarning}>
              ⚠ Output truncated
            </div>
          )}
          <pre className={`${STYLES.output.streamPre} ${
            isStderr ? STYLES.output.streamStderr : STYLES.output.streamStdout
          }`}>{displayText}</pre>
        </div>
        {truncated && (
          <button
            className={STYLES.output.expandButton}
            onClick={() => setExpanded(!expanded)}
          >
            Show more ({totalLines - MAX_LINES} more lines)
          </button>
        )}
        {expanded && (
          <button
            className={STYLES.output.expandButton}
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
        <div className={STYLES.output.imageContainer}>
          <img
            src={`data:image/png;base64,${data["image/png"]}`}
            alt="Output"
            className={STYLES.output.image}
          />
        </div>
      );
    }

    // Images (JPEG)
    if (data["image/jpeg"]) {
      return (
        <div className={STYLES.output.imageContainer}>
          <img
            src={`data:image/jpeg;base64,${data["image/jpeg"]}`}
            alt="Output"
            className={STYLES.output.image}
          />
        </div>
      );
    }

    // SVG
    if (data["image/svg+xml"]) {
      const svgContent = Array.isArray(data["image/svg+xml"])
        ? data["image/svg+xml"].join("")
        : data["image/svg+xml"];
      return (
        <div className={STYLES.output.svgContainer}>
          {/* Note: SVG content is rendered directly. For untrusted notebooks, consider sandboxing. */}
          <div dangerouslySetInnerHTML={{ __html: svgContent }} />
        </div>
      );
    }

    // HTML (DataFrames, matplotlib HTML output, etc.)
    if (data["text/html"]) {
      const htmlContent = Array.isArray(data["text/html"])
        ? data["text/html"].join("")
        : data["text/html"];
      return (
        <div className={STYLES.output.dataframeWrapper}>
          <div className={STYLES.output.dataframe}>
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
        <div className={STYLES.output.plainWrapper}>
          <div className={STYLES.output.plainContent}>
            {isTruncatedByBackend && (
              <div className={STYLES.output.truncateWarning}>
                ⚠ Output truncated
              </div>
            )}
            <pre className={STYLES.output.plainPre}>{displayText}</pre>
          </div>
          {truncated && (
            <button
              className={STYLES.output.expandButton}
              onClick={() => setExpanded(!expanded)}
            >
              Show more ({totalLines - MAX_LINES} more lines)
            </button>
          )}
          {expanded && (
            <button
              className={STYLES.output.expandButton}
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
      <div className={STYLES.output.rawData}>
        <pre className={STYLES.output.plainPre}>{JSON.stringify(data, null, 2)}</pre>
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
      <div className={STYLES.output.errorContainer}>
        <div className={STYLES.output.errorContent}>
          {isTruncatedByBackend && (
            <div className={STYLES.output.truncateWarning}>
              ⚠ Output truncated
            </div>
          )}
          <pre className={STYLES.output.errorPre}>{displayText}</pre>
        </div>
        {truncated && (
          <button
            className={STYLES.output.expandButton}
            onClick={() => setExpanded(!expanded)}
          >
            Show more ({totalLines - MAX_LINES} more lines)
          </button>
        )}
        {expanded && (
          <button
            className={STYLES.output.expandButton}
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
  const [runningCellId, setRunningCellId] = useState(null);
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
  const [showSecretsWarning, setShowSecretsWarning] = useState(false);
  const [cellsWithSecrets, setCellsWithSecrets] = useState([]);
  const [hasSecretsInOutputs, setHasSecretsInOutputs] = useState(false);
  const contentScrollRef = useRef(null); // Ref for scroll container
  const [swappingCells, setSwappingCells] = useState(null); // Track cells being swapped { from, to }

  useEffect(() => {
    loadWorkbook();
    startEngine();

    return () => {
      // Cleanup: stop engine when component unmounts
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

  // Listen for save-all event from parent
  useEffect(() => {
    const handleSaveAll = () => {
      if (hasUnsavedChanges) {
        console.log("Saving workbook in response to save-all event");
        saveWorkbook(true); // Skip secrets check for auto-save
      }
    };

    window.addEventListener("tether:save-all", handleSaveAll);
    return () => window.removeEventListener("tether:save-all", handleSaveAll);
  }, [hasUnsavedChanges]);

  // Scan for secrets whenever notebook changes
  useEffect(() => {
    const scanForSecrets = async () => {
      if (!notebook || !projectRoot) return;

      try {
        const cellsJson = JSON.stringify(notebook.cells);
        const scanResult = await invoke("scan_outputs_for_secrets", {
          projectPath: projectRoot,
          cellsJson: cellsJson,
        });

        setHasSecretsInOutputs(scanResult.has_secrets);
        setCellsWithSecrets(scanResult.cell_indices);
      } catch (err) {
        // On error, assume no secrets (silently fail)
        setHasSecretsInOutputs(false);
        setCellsWithSecrets([]);
      }
    };

    scanForSecrets();
  }, [notebook, projectRoot]);

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
      console.log("Engine already started, skipping");
      return;
    }

    try {
      console.log("Starting engine for workbook:", workbookPath);
      console.log("Project root:", projectRoot);
      setEngineStatus('starting');
      setError(null);

      // Add timeout to prevent infinite hanging (60 seconds to allow for venv setup and package installation)
      const timeoutPromise = new Promise((_, reject) =>
        setTimeout(() => reject(new Error("Engine startup timed out after 60 seconds. This may indicate:\n- Python environment setup is slow\n- Required packages need to be installed\n- Jupyter kernel is not responding\n\nCheck logs (View → Show Runtime Logs) for details.")), 60000)
      );

      const startPromise = invoke("start_engine", {
        workbookPath: workbookPath,
        projectPath: projectRoot,
        engineName: null,  // Auto-detect
      });

      await Promise.race([startPromise, timeoutPromise]);

      console.log("Engine started successfully");
      engineStartedRef.current = true;
      setEngineReady(true);
      setEngineStatus('idle');

    } catch (err) {
      console.error("Failed to start engine:", err);
      const errorMsg = typeof err === 'string' ? err : err.message || "Unknown error";
      setEngineStatus('error');
      setError(`Failed to start engine: ${errorMsg}`);
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
      setRunningCellId(null);
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
      // Ensure all cells have unique IDs for proper React rendering
      const notebookWithIds = {
        ...parsed,
        cells: ensureCellIds(parsed.cells)
      };
      setNotebook(notebookWithIds);
      setHasUnsavedChanges(false);
    } catch (err) {
      console.error("Failed to load notebook:", err);
      setError(err.toString());
    } finally {
      setLoading(false);
    }
  };

  const saveWorkbook = async (skipSecretsCheck = false) => {
    if (!notebook) return;

    // If secrets are in outputs and we're not skipping the check, show modal instead of saving
    if (!skipSecretsCheck && hasSecretsInOutputs) {
      setShowSecretsWarning(true);
      return; // Don't save yet - wait for user action from modal
    }

    try {

      // No secrets detected (or check was skipped), proceed with save
      const notebookToSave = {
        ...notebook,
        cells: notebook.cells.map(cell => ({
          ...cell,
          // Ensure source is always an array of strings
          source: Array.isArray(cell.source) ? cell.source : [cell.source],
          // Redact outputs that contain secrets (backward compatibility)
          outputs: cell.outputs ? cell.outputs.map(output => {
            // If output contains secrets, replace with redacted message
            if (output.metadata && output.metadata.contains_secrets === true) {
              return {
                output_type: "stream",
                name: "stdout",
                text: "[secret hidden here]\n",
                metadata: { contains_secrets: true }
              };
            }
            return output;
          }) : cell.outputs,
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

  const handleClearAndSave = async () => {
    // Clear outputs from cells that contain secrets
    setNotebook(prevNotebook => {
      const newCells = prevNotebook.cells.map((cell, index) => {
        if (cellsWithSecrets.includes(index)) {
          return {
            ...cell,
            outputs: [],
          };
        }
        return cell;
      });
      return { ...prevNotebook, cells: newCells };
    });

    // Close modal
    setShowSecretsWarning(false);
    setCellsWithSecrets([]);

    // Save with skip check since we just cleared the outputs
    setTimeout(() => {
      saveWorkbook(true);
    }, 100); // Small delay to ensure state update
  };

  const handleGoBack = () => {
    // Just close the modal and let user fix manually
    setShowSecretsWarning(false);
    setCellsWithSecrets([]);
  };

  const handleDangerouslySave = async () => {
    // Close modal
    setShowSecretsWarning(false);
    setCellsWithSecrets([]);

    // Save without secrets check
    await saveWorkbook(true);
  };

  const updateCell = (index, newContent) => {
    // Use functional update to prevent race conditions
    setNotebook(prevNotebook => {
      const newCells = [...prevNotebook.cells];
      const oldSource = newCells[index].source.join("");

      // Only mark as dirty if content actually changed
      if (oldSource !== newContent) {
        newCells[index] = {
          ...newCells[index],
          source: newContent.split("\n").map((line, i, arr) =>
            i < arr.length - 1 ? line + "\n" : line
          ),
        };
        setHasUnsavedChanges(true);
        return { ...prevNotebook, cells: newCells };
      }

      return prevNotebook;
    });
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
    if (!notebook || !notebook.cells || index >= notebook.cells.length) {
      console.error("Invalid index or notebook state in moveCellUp", { index, cellCount: notebook?.cells?.length });
      return;
    }

    // Prevent cell movement while code is running
    if (runningCellId) {
      console.warn("Cannot move cells while code is executing");
      return;
    }

    try {
      // Capture scroll position before swapping
      const scrollY = contentScrollRef.current?.scrollTop || 0;

      // Mark cells as swapping to hide their editors
      const cellId1 = notebook.cells[index - 1]?.metadata?.cell_id;
      const cellId2 = notebook.cells[index]?.metadata?.cell_id;
      setSwappingCells({ cellId1, cellId2 });

      // Small delay to let CSS hide the editors
      setTimeout(() => {
        // Batch all state updates together
        setNotebook(prevNotebook => {
          if (!prevNotebook || !prevNotebook.cells || index === 0 || index >= prevNotebook.cells.length) {
            console.error("Invalid state in moveCellUp");
            return prevNotebook;
          }

          const newCells = [...prevNotebook.cells];
          // Swap cells
          const temp = newCells[index];
          newCells[index] = newCells[index - 1];
          newCells[index - 1] = temp;
          return { ...prevNotebook, cells: newCells };
        });

        // Update selected cell to follow the moved cell
        setSelectedCell(prev => index === prev ? index - 1 : prev);
        setHasUnsavedChanges(true);

        // Restore scroll and unhide editors after swap completes
        requestAnimationFrame(() => {
          if (contentScrollRef.current) {
            contentScrollRef.current.scrollTop = scrollY;
          }
          setSwappingCells(null);
        });
      }, 10);
    } catch (error) {
      console.error("Error in moveCellUp:", error);
      setSwappingCells(null);
    }
  };

  const moveCellDown = (index) => {
    if (!notebook || !notebook.cells || index < 0 || index >= notebook.cells.length - 1) {
      console.error("Invalid index or notebook state in moveCellDown", { index, cellCount: notebook?.cells?.length });
      return;
    }

    // Prevent cell movement while code is running
    if (runningCellId) {
      console.warn("Cannot move cells while code is executing");
      return;
    }

    try {
      // Capture scroll position before swapping
      const scrollY = contentScrollRef.current?.scrollTop || 0;

      // Mark cells as swapping to hide their editors
      const cellId1 = notebook.cells[index]?.metadata?.cell_id;
      const cellId2 = notebook.cells[index + 1]?.metadata?.cell_id;
      setSwappingCells({ cellId1, cellId2 });

      // Small delay to let CSS hide the editors
      setTimeout(() => {
        // Batch all state updates together
        setNotebook(prevNotebook => {
          if (!prevNotebook || !prevNotebook.cells || index < 0 || index >= prevNotebook.cells.length - 1) {
            console.error("Invalid state in moveCellDown");
            return prevNotebook;
          }

          const newCells = [...prevNotebook.cells];
          // Swap cells
          const temp = newCells[index];
          newCells[index] = newCells[index + 1];
          newCells[index + 1] = temp;
          return { ...prevNotebook, cells: newCells };
        });

        // Update selected cell to follow the moved cell
        setSelectedCell(prev => index === prev ? index + 1 : prev);
        setHasUnsavedChanges(true);

        // Restore scroll and unhide editors after swap completes
        requestAnimationFrame(() => {
          if (contentScrollRef.current) {
            contentScrollRef.current.scrollTop = scrollY;
          }
          setSwappingCells(null);
        });
      }, 10);
    } catch (error) {
      console.error("Error in moveCellDown:", error);
      setSwappingCells(null);
    }
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
      metadata: {
        cell_id: generateCellId()
      },
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
          metadata: {
            cell_id: generateCellId()
          },
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
            metadata: {
              cell_id: generateCellId()
            },
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
          metadata: {
            cell_id: generateCellId()
          },
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
            metadata: {
              cell_id: generateCellId()
            },
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
      setRunningCellId(cell.metadata?.cell_id || null);

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
      // Store cell ID to find the correct cell even if cells are reordered
      const executingCellId = cell.metadata.cell_id;
      const unlisten = await listen(eventName, (event) => {
        const output = event.payload;

        // Add output to cell progressively
        setNotebook(prevNotebook => {
          const newCells = [...prevNotebook.cells];
          // Find cell by ID instead of index to handle cell reordering
          const cellIndex = newCells.findIndex(c => c.metadata?.cell_id === executingCellId);
          if (cellIndex === -1) {
            console.warn("Could not find executing cell - it may have been deleted");
            return prevNotebook;
          }
          const currentCell = newCells[cellIndex];

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
          metadata: {
            cell_id: generateCellId()
          },
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
            metadata: {
              cell_id: generateCellId()
            },
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
      setRunningCellId(null);

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
      setRunningCellId(null);
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
      // Snapshot cell IDs at the start to prevent corruption if cells are reordered during execution
      const cellsToExecute = notebook.cells
        .filter(cell => cell.cell_type === "code" && cell.source.join("").trim() && cell.metadata?.cell_id)
        .map(cell => ({
          id: cell.metadata.cell_id,
          code: cell.source.join("")
        }));

      // Execute all code cells in sequence
      for (const { id, code } of cellsToExecute) {
        setRunningCellId(id);

        // Clear the output before running to show it's executing
        setNotebook(prevNotebook => {
          const newCells = [...prevNotebook.cells];
          const idx = newCells.findIndex(c => c.metadata?.cell_id === id);
          if (idx !== -1) {
            newCells[idx].outputs = [];
          }
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
          const idx = newCells.findIndex(c => c.metadata?.cell_id === id);
          if (idx !== -1) {
            newCells[idx].outputs = result.outputs || [];

            // Set execution_count from the result
            if (result.execution_count !== null && result.execution_count !== undefined) {
              newCells[idx].execution_count = result.execution_count;
            }
          }
          return { ...prevNotebook, cells: newCells };
        });
        setHasUnsavedChanges(true);
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
      setRunningCellId(null);
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
      metadata: {
        cell_id: generateCellId()
      },
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
    <div className={STYLES.viewer.container}>
      <div className={STYLES.viewer.header}>
        <div className={STYLES.viewer.headerTop}>
          <h2 className={STYLES.viewer.title}>
            {getWorkbookName()}
            {hasUnsavedChanges && <span className={STYLES.viewer.unsavedDot}>•</span>}
          </h2>
          <div className={STYLES.viewer.headerControls}>
            <span className={`${STYLES.viewer.statusBadge} ${
              engineStatus === 'starting' ? STYLES.viewer.statusStarting :
              engineStatus === 'idle' ? STYLES.viewer.statusIdle :
              engineStatus === 'busy' ? STYLES.viewer.statusBusy :
              engineStatus === 'restarting' ? STYLES.viewer.statusRestarting :
              STYLES.viewer.statusError
            }`} title={getEngineStatusText()}>
              {getEngineStatusText()}
            </span>
            {hasUnsavedChanges && (
              <button
                onClick={() => saveWorkbook()}
                className={`${STYLES.viewer.saveButton} ${
                  hasSecretsInOutputs
                    ? STYLES.viewer.saveWarning
                    : STYLES.viewer.saveNormal
                }`}
                title={hasSecretsInOutputs ? "Secrets detected in outputs - click to review" : "Save notebook"}
              >
                {hasSecretsInOutputs ? '⚠ Save' : 'Save'}
              </button>
            )}
          </div>
        </div>
        <div className={STYLES.viewer.toolbar}>
          <span className={STYLES.viewer.hint}>Shift+Enter to run</span>
          <div className={STYLES.viewer.spacer} />

          {/* Execution controls group */}
          <div className={STYLES.viewer.buttonGroup}>
            {engineStatus === 'error' && (
              <button
                onClick={startEngine}
                className={STYLES.viewer.buttonPrimary}
                title="Retry connecting to engine"
              >
                🔌 Reconnect
              </button>
            )}
            <button
              onClick={runAllCells}
              disabled={isRunningAll || !isEngineReady}
              className={STYLES.viewer.button}
              title="Run all cells"
            >
              {isRunningAll ? "⏳ Running..." : "▶ Run All"}
            </button>
            <button
              onClick={interruptExecution}
              disabled={engineStatus !== 'busy'}
              className={STYLES.viewer.button}
              title="Interrupt execution"
            >
              ⏹ Interrupt
            </button>
          </div>

          <div className={STYLES.viewer.separator}></div>

          {/* Kernel controls group */}
          <div className={STYLES.viewer.buttonGroup}>
            <button
              onClick={clearAllOutputs}
              className={STYLES.viewer.button}
              title="Clear all outputs"
            >
              🗙 Clear
            </button>
            <button
              onClick={restartEngine}
              disabled={!isEngineReady}
              className={STYLES.viewer.button}
              title="Restart kernel"
            >
              🔄 Restart
            </button>
          </div>

          <div className={STYLES.viewer.separator}></div>

          {/* Add cell controls group */}
          <div className={STYLES.viewer.buttonGroup}>
            <button
              onClick={() => addCell("markdown")}
              className={STYLES.viewer.button}
              title="Add markdown cell"
            >
              + Markdown
            </button>
            <button
              onClick={() => addCell("code")}
              className={STYLES.viewer.button}
              title="Add code cell"
            >
              + Code
            </button>
          </div>
        </div>
      </div>

      <div ref={contentScrollRef} className={STYLES.viewer.content}>
        {error && (
          <div className={STYLES.viewer.error}>
            <span>{error}</span>
            <button
              onClick={() => setError(null)}
              className={STYLES.viewer.errorClose}
              title="Dismiss error"
            >
              ×
            </button>
          </div>
        )}
        {notebook.cells.length === 0 && (
          <div className={STYLES.viewer.emptyState}>
            <p>Empty notebook. Add a cell to get started.</p>
          </div>
        )}
        {notebook.cells.map((cell, index) => (
          <WorkbookCell
            key={cell.metadata?.cell_id || index}
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
            isRunning={runningCellId === cell.metadata?.cell_id}
            executionElapsed={runningCellId === cell.metadata?.cell_id ? cellExecutionElapsed : 0}
            onSelect={setSelectedCell}
            onEnterEditMode={() => {
              setSelectedCell(index);
              setEditMode(true);
            }}
            autosaveEnabled={autosaveEnabled}
            swappingCells={swappingCells}
          />
        ))}
      </div>

      {/* Secrets Warning Modal */}
      {showSecretsWarning && (
        <SecretsWarningModal
          cellIndices={cellsWithSecrets}
          onClearAndSave={handleClearAndSave}
          onGoBack={handleGoBack}
          onDangerouslySave={handleDangerouslySave}
        />
      )}
    </div>
  );
}
