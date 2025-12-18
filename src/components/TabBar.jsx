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
      <div className="flex items-center justify-between bg-gray-50 border-b border-gray-200 h-9 px-3">
        <div className="flex items-center gap-3 pl-3 border-l border-gray-200">
          <label className="flex items-center gap-2 text-xs text-gray-900 cursor-pointer select-none">
            <input
              type="checkbox"
              checked={autosaveEnabled}
              onChange={(e) => onAutosaveToggle(e.target.checked)}
              className="w-3.5 h-3.5 cursor-pointer"
            />
            <span className="font-medium">Autosave</span>
          </label>
        </div>
      </div>
    );
  }

  return (
    <div className="flex items-center justify-between bg-gray-50 border-b border-gray-200 h-9">
      <div className="flex overflow-x-auto overflow-y-hidden flex-1 min-w-0 custom-scrollbar">
        {tabs.map((tab) => (
          <div
            key={tab.id}
            className={`flex items-center gap-1.5 px-3 h-9 border-r border-gray-200 cursor-pointer min-w-[120px] max-w-[200px] transition-colors ${
              tab.id === activeTabId ? "bg-white" : "bg-gray-100 hover:bg-gray-200"
            }`}
            onClick={() => onTabSelect(tab.id)}
          >
            <span className="text-sm flex-shrink-0">{getFileIcon(tab.path, tab.type)}</span>
            <span className="flex-1 overflow-hidden text-ellipsis whitespace-nowrap text-sm">
              {getFileName(tab.path)}
            </span>
            {tab.hasUnsavedChanges && <span className="text-amber-500 text-base leading-none flex-shrink-0">•</span>}
            <button
              className="flex items-center justify-center w-5 h-5 rounded text-gray-600 hover:bg-gray-300/50 hover:text-gray-900 text-lg leading-none p-0 flex-shrink-0 transition-colors"
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
      <div className="flex items-center gap-3 px-3 pl-3 border-l border-gray-200 flex-shrink-0">
        <label className="flex items-center gap-2 text-xs text-gray-900 cursor-pointer select-none">
          <input
            type="checkbox"
            checked={autosaveEnabled}
            onChange={(e) => onAutosaveToggle(e.target.checked)}
            className="w-3.5 h-3.5 cursor-pointer"
          />
          <span className="font-medium">Autosave</span>
        </label>
      </div>
    </div>
  );
}
