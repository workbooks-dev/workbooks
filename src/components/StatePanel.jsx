import { useState, useEffect } from "react";
import { useTether } from "../hooks/useTether";

export function StatePanel() {
  const [stateVars, setStateVars] = useState([]);
  const [selectedVar, setSelectedVar] = useState(null);
  const { getState, inspectStateVariable } = useTether();

  useEffect(() => {
    loadState();
  }, []);

  const loadState = async () => {
    try {
      const state = await getState();
      setStateVars(state);
    } catch (error) {
      console.error("Failed to load state:", error);
    }
  };

  const handleInspect = async (key) => {
    try {
      const variable = await inspectStateVariable(key);
      setSelectedVar(variable);
    } catch (error) {
      console.error("Failed to inspect variable:", error);
    }
  };

  return (
    <div className="state-panel">
      <h2>State Variables</h2>
      <div className="state-list">
        {stateVars.map((sv) => (
          <div
            key={sv.key}
            className="state-item"
            onClick={() => handleInspect(sv.key)}
          >
            <div className="state-key">{sv.key}</div>
            <div className="state-type">{sv.type}</div>
            {sv.size && <div className="state-size">{sv.size} bytes</div>}
          </div>
        ))}
      </div>
      {selectedVar && (
        <div className="state-inspector">
          <h3>{selectedVar.key}</h3>
          <pre>{JSON.stringify(selectedVar, null, 2)}</pre>
        </div>
      )}
    </div>
  );
}
