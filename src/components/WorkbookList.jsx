import { useState, useEffect } from "react";
import { useWorkbooks } from "../hooks/useWorkbooks";

export function NotebookList() {
  const [notebooks, setNotebooks] = useState([]);
  const { listNotebooks, runNotebook } = useWorkbooks();

  useEffect(() => {
    loadNotebooks();
  }, []);

  const loadNotebooks = async () => {
    try {
      const nbs = await listNotebooks();
      setNotebooks(nbs);
    } catch (error) {
      console.error("Failed to load notebooks:", error);
    }
  };

  const handleRun = async (notebook) => {
    try {
      await runNotebook(notebook.path);
      await loadNotebooks();
    } catch (error) {
      console.error("Failed to run notebook:", error);
    }
  };

  return (
    <div className="notebook-list">
      <h2>Notebooks</h2>
      <div className="notebooks">
        {notebooks.map((nb) => (
          <div key={nb.path} className="notebook-item">
            <div className="notebook-info">
              <h3>{nb.name}</h3>
              <span className={`status status-${nb.status}`}>{nb.status}</span>
            </div>
            <button onClick={() => handleRun(nb)}>Run</button>
          </div>
        ))}
      </div>
    </div>
  );
}
