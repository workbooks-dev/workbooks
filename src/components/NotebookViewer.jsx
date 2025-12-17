import { useState, useEffect, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import Editor from "@monaco-editor/react";

function NotebookCell({ cell, index, onUpdate, onDelete, onExecute, onMoveUp, onMoveDown, isSelected, isEditMode, onSelect, onEnterEditMode }) {
  const [content, setContent] = useState(cell.source.join(""));
  const editorRef = useRef(null);
  const initialSourceRef = useRef(cell.source.join(""));

  useEffect(() => {
    // Only update content if the cell.source actually changed from external source
    // (not from our own edits)
    const newSource = cell.source.join("");
    if (newSource !== initialSourceRef.current && newSource !== content) {
      setContent(newSource);
      initialSourceRef.current = newSource;
    }
  }, [cell.source]);

  const handleEditorChange = (value) => {
    console.log(`[Cell ${index}] handleEditorChange - new value:`, value);
    console.log(`[Cell ${index}] handleEditorChange - old content:`, content);
    setContent(value || "");
    console.log(`[Cell ${index}] handleEditorChange - state should now be:`, value || "");
  };

  const handleExecute = () => {
    // Get the current value directly from the editor to avoid stale state
    const currentValue = editorRef.current ? editorRef.current.getValue() : content;
    console.log(`[Cell ${index}] handleExecute - content state:`, content);
    console.log(`[Cell ${index}] handleExecute - editor value:`, currentValue);
    console.log(`[Cell ${index}] handleExecute - cell.source:`, cell.source.join(""));

    // Update local state to match (though it might already be updating)
    if (currentValue !== content) {
      setContent(currentValue);
    }

    // Update the cell source before executing
    onUpdate(index, currentValue);
    onExecute(index, currentValue);
  };

  const handleBlur = () => {
    // Get current value from editor to avoid stale state
    const currentValue = editorRef.current ? editorRef.current.getValue() : content;

    // Save changes when cell loses focus
    if (currentValue !== cell.source.join("")) {
      setContent(currentValue);
      onUpdate(index, currentValue);
    }
  };

  const handleKeyDown = (e) => {
    if (e.shiftKey && e.key === "Enter") {
      e.preventDefault();
      handleExecute();
    }
  };

  // Update source when edit mode changes (user navigates away)
  useEffect(() => {
    if (!isEditMode) {
      const currentValue = editorRef.current ? editorRef.current.getValue() : content;
      if (currentValue !== cell.source.join("")) {
        setContent(currentValue);
        onUpdate(index, currentValue);
      }
    }
  }, [isEditMode]);

  if (cell.cell_type === "markdown") {
    return (
      <div
        className={`notebook-cell markdown-cell ${isSelected ? "selected" : ""} ${isEditMode ? "edit-mode" : ""}`}
        onClick={() => onSelect(index)}
      >
        {isSelected && (
          <div className="cell-toolbar">
            <button onClick={(e) => { e.stopPropagation(); onMoveUp(index); }} title="Move up">↑</button>
            <button onClick={(e) => { e.stopPropagation(); onMoveDown(index); }} title="Move down">↓</button>
            <button onClick={(e) => { e.stopPropagation(); onDelete(index); }} title="Delete">🗑</button>
          </div>
        )}
        <div className="cell-input">
          <textarea
            value={content}
            onChange={(e) => handleEditorChange(e.target.value)}
            onKeyDown={handleKeyDown}
            onBlur={handleBlur}
            className="markdown-editor"
            placeholder="Enter markdown..."
            rows={Math.max(3, content.split("\n").length)}
          />
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
            <button onClick={(e) => { e.stopPropagation(); onMoveUp(index); }} title="Move up">↑</button>
            <button onClick={(e) => { e.stopPropagation(); onMoveDown(index); }} title="Move down">↓</button>
            <button onClick={(e) => { e.stopPropagation(); onDelete(index); }} title="Delete">🗑</button>
          </div>
        )}
        <div className="cell-prompt">
          [{cell.execution_count || " "}]
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
                editor.onKeyDown((e) => {
                  if (e.shiftKey && e.keyCode === 3) {
                    // Shift+Enter (keyCode 3 is Enter in Monaco)
                    e.preventDefault();
                    handleExecute();
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
  // Strip ANSI color codes from text
  const stripAnsi = (text) => {
    if (!text) return text;
    // Remove ANSI escape sequences
    return text.replace(/\x1b\[[0-9;]*m/g, '');
  };

  if (output.output_type === "stream") {
    const text = Array.isArray(output.text) ? output.text.join("") : output.text;
    const className = output.name === "stderr" ? "output-stderr" : "output-stdout";
    return (
      <div className={`cell-output ${className}`}>
        <pre>{stripAnsi(text)}</pre>
      </div>
    );
  }

  if (output.output_type === "execute_result") {
    const text = output.data["text/plain"] || JSON.stringify(output.data);
    return (
      <div className="cell-output output-result">
        <pre>{stripAnsi(text)}</pre>
      </div>
    );
  }

  if (output.output_type === "error") {
    const traceback = output.traceback ? output.traceback.join("\n") : output.evalue;
    return (
      <div className="cell-output output-error">
        <pre>{stripAnsi(traceback)}</pre>
      </div>
    );
  }

  return null;
}

export function NotebookViewer({ notebookPath, projectRoot, onClose }) {
  const [notebook, setNotebook] = useState(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState(null);
  const [hasUnsavedChanges, setHasUnsavedChanges] = useState(false);
  const [selectedCell, setSelectedCell] = useState(0);
  const [editMode, setEditMode] = useState(false);
  const kernelStartedRef = useRef(false);
  const cellRefs = useRef([]);

  useEffect(() => {
    loadNotebook();
    startKernel();

    return () => {
      // Cleanup: stop kernel when component unmounts
      stopKernel();
    };
  }, [notebookPath]);

  useEffect(() => {
    // Global keyboard shortcuts
    const handleKeyDown = (e) => {
      // Cmd/Ctrl+S to save
      if ((e.metaKey || e.ctrlKey) && e.key === "s") {
        e.preventDefault();
        saveNotebook();
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
        // D+D to delete cell
        else if (e.key === "d" && e.shiftKey) {
          e.preventDefault();
          deleteCell(selectedCell);
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

  const startKernel = async () => {
    try {
      console.log("Starting kernel for notebook:", notebookPath);
      console.log("Project root:", projectRoot);
      console.log("Current kernelStarted state before invoke:", kernelStartedRef.current);

      await invoke("start_kernel", {
        notebookPath: notebookPath,
        projectPath: projectRoot,
        kernelName: null,  // Auto-detect
      });

      console.log("Kernel started successfully, setting kernelStarted to true");
      kernelStartedRef.current = true;
      console.log("kernelStarted ref should now be true");
    } catch (err) {
      console.error("Failed to start kernel:", err);
      setError(`Failed to start kernel: ${err}`);
      kernelStartedRef.current = false;
    }
  };

  const stopKernel = async () => {
    try {
      console.log("Stopping kernel for notebook:", notebookPath);
      kernelStartedRef.current = false;
      await invoke("stop_kernel", {
        notebookPath: notebookPath,
      });
      console.log("Kernel stopped successfully");
    } catch (err) {
      console.error("Failed to stop kernel:", err);
    }
  };

  const loadNotebook = async () => {
    setLoading(true);
    setError(null);

    try {
      const content = await invoke("read_notebook", {
        notebookPath: notebookPath,
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

  const saveNotebook = async () => {
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

      console.log("Saving notebook:", notebookToSave);
      const content = JSON.stringify(notebookToSave, null, 2);
      await invoke("save_notebook", {
        notebookPath: notebookPath,
        content: content,
      });
      setHasUnsavedChanges(false);
      console.log("Notebook saved successfully");
    } catch (err) {
      console.error("Failed to save notebook:", err);
      setError(err.toString());
    }
  };

  const updateCell = (index, newContent) => {
    console.log(`[updateCell] index: ${index}, newContent:`, newContent);
    const newCells = [...notebook.cells];
    newCells[index] = {
      ...newCells[index],
      source: newContent.split("\n").map((line, i, arr) =>
        i < arr.length - 1 ? line + "\n" : line
      ),
    };
    console.log(`[updateCell] Updated cell.source:`, newCells[index].source.join(""));
    setNotebook({ ...notebook, cells: newCells });
    setHasUnsavedChanges(true);
  };

  const deleteCell = (index) => {
    if (notebook.cells.length === 1) {
      // Don't delete the last cell
      return;
    }
    const newCells = notebook.cells.filter((_, i) => i !== index);
    setNotebook({ ...notebook, cells: newCells });
    setHasUnsavedChanges(true);
    // Adjust selected cell if needed
    if (selectedCell >= newCells.length) {
      setSelectedCell(Math.max(0, newCells.length - 1));
    }
  };

  const moveCellUp = (index) => {
    if (index === 0) return;
    const newCells = [...notebook.cells];
    [newCells[index - 1], newCells[index]] = [newCells[index], newCells[index - 1]];
    setNotebook({ ...notebook, cells: newCells });
    setSelectedCell(index - 1);
    setHasUnsavedChanges(true);
  };

  const moveCellDown = (index) => {
    if (index === notebook.cells.length - 1) return;
    const newCells = [...notebook.cells];
    [newCells[index], newCells[index + 1]] = [newCells[index + 1], newCells[index]];
    setNotebook({ ...notebook, cells: newCells });
    setSelectedCell(index + 1);
    setHasUnsavedChanges(true);
  };

  const changeCellType = (index, newType) => {
    const newCells = [...notebook.cells];
    const cell = newCells[index];
    if (cell.cell_type === newType) return;

    cell.cell_type = newType;
    if (newType === "code") {
      cell.execution_count = null;
      cell.outputs = [];
    } else {
      delete cell.execution_count;
      delete cell.outputs;
    }
    setNotebook({ ...notebook, cells: newCells });
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

    const newCells = [...notebook.cells];
    newCells.splice(index, 0, newCell);
    setNotebook({ ...notebook, cells: newCells });
    setSelectedCell(index);
    setEditMode(true);
    setHasUnsavedChanges(true);
  };

  const executeCell = async (index, code) => {
    console.log("executeCell called, kernelStarted:", kernelStartedRef.current);
    console.log("Executing code:", code);

    if (!kernelStartedRef.current) {
      console.error("Kernel not started! kernelStarted ref is false");
      setError("Kernel not started");
      return;
    }

    try {
      const result = await invoke("execute_cell", {
        notebookPath: notebookPath,
        code: code,
      });
      console.log("Cell execution result:", result);

      // Create new cells array without modifying cell.source
      // (it was already updated by handleExecute -> onUpdate)
      const newCells = [...notebook.cells];
      const cell = newCells[index];

      // Only update outputs and execution count
      cell.outputs = result.outputs || [];

      // Update execution count - use from result if available, otherwise increment
      const executionCount = result.outputs?.find(o => o.output_type === 'execute_result')?.execution_count;
      if (executionCount) {
        cell.execution_count = executionCount;
      } else {
        cell.execution_count = (cell.execution_count || 0) + 1;
      }

      setNotebook({ ...notebook, cells: newCells });
      setHasUnsavedChanges(true);

      // Move to next cell after execution, or create a new one if at the end
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
        newCells.push(newCell);
        setNotebook({ ...notebook, cells: newCells });
        setSelectedCell(index + 1);
        setEditMode(true);
      }
    } catch (err) {
      console.error("Failed to execute cell:", err);
      setError(err.toString());
    }
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

    const newCells = [...notebook.cells, newCell];
    setNotebook({ ...notebook, cells: newCells });
    setHasUnsavedChanges(true);
  };

  const getNotebookName = () => {
    return notebookPath.split("/").pop();
  };

  if (loading) {
    return (
      <div className="notebook-viewer loading">
        <p>Loading notebook...</p>
      </div>
    );
  }

  if (error) {
    return (
      <div className="notebook-viewer error">
        <p>Error: {error}</p>
        <button onClick={onClose}>Close</button>
      </div>
    );
  }

  if (!notebook) {
    return null;
  }

  return (
    <div className="notebook-viewer">
      <div className="notebook-header">
        <h2>
          {getNotebookName()}
          {hasUnsavedChanges && <span className="unsaved-indicator"> •</span>}
        </h2>
        <div className="notebook-actions">
          <span className="keyboard-hint">Shift+Enter to run</span>
          <button onClick={() => addCell("markdown")}>+ Markdown</button>
          <button onClick={() => addCell("code")}>+ Code</button>
          <button onClick={saveNotebook} className={hasUnsavedChanges ? "primary" : ""}>
            Save {hasUnsavedChanges && "*"}
          </button>
          <button onClick={onClose}>Close</button>
        </div>
      </div>

      <div className="notebook-cells">
        {notebook.cells.length === 0 && (
          <div className="empty-notebook">
            <p>Empty notebook. Add a cell to get started.</p>
          </div>
        )}
        {notebook.cells.map((cell, index) => (
          <NotebookCell
            key={index}
            cell={cell}
            index={index}
            onUpdate={updateCell}
            onDelete={deleteCell}
            onExecute={executeCell}
            onMoveUp={moveCellUp}
            onMoveDown={moveCellDown}
            isSelected={selectedCell === index}
            isEditMode={editMode && selectedCell === index}
            onSelect={setSelectedCell}
            onEnterEditMode={() => {
              setSelectedCell(index);
              setEditMode(true);
            }}
          />
        ))}
      </div>
    </div>
  );
}
