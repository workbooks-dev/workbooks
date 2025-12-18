export function TabBar({ tabs, activeTabId, onTabSelect, onTabClose, autosaveEnabled, onAutosaveToggle }) {
  const getFileName = (path) => {
    return path.split("/").pop();
  };

  const getFileIcon = (path, type) => {
    if (type === "notebook") {
      return "📓";
    }

    const ext = path.split(".").pop()?.toLowerCase();
    switch (ext) {
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

  if (tabs.length === 0) {
    return (
      <div className="tab-bar">
        <div className="tab-bar-controls">
          <label className="autosave-toggle">
            <input
              type="checkbox"
              checked={autosaveEnabled}
              onChange={(e) => onAutosaveToggle(e.target.checked)}
            />
            <span>Autosave</span>
          </label>
        </div>
      </div>
    );
  }

  return (
    <div className="tab-bar">
      <div className="tab-bar-tabs">
        {tabs.map((tab) => (
          <div
            key={tab.id}
            className={`tab ${tab.id === activeTabId ? "active" : ""} ${
              tab.hasUnsavedChanges ? "unsaved" : ""
            }`}
            onClick={() => onTabSelect(tab.id)}
          >
            <span className="tab-icon">{getFileIcon(tab.path, tab.type)}</span>
            <span className="tab-name">{getFileName(tab.path)}</span>
            {tab.hasUnsavedChanges && <span className="tab-unsaved-dot">•</span>}
            <button
              className="tab-close"
              onClick={(e) => {
                e.stopPropagation();
                onTabClose(tab.id);
              }}
              title="Close"
            >
              ×
            </button>
          </div>
        ))}
      </div>
      <div className="tab-bar-controls">
        <label className="autosave-toggle">
          <input
            type="checkbox"
            checked={autosaveEnabled}
            onChange={(e) => onAutosaveToggle(e.target.checked)}
          />
          <span>Autosave</span>
        </label>
      </div>
    </div>
  );
}
