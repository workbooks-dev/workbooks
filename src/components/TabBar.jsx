export function TabBar({ tabs, activeTabId, onTabSelect, onTabClose }) {
  const getFileName = (path, name) => {
    // Use custom name if provided (e.g., "Secrets" for secrets tab)
    if (name) return name;
    return path.split("/").pop();
  };

  if (tabs.length === 0) {
    return null;
  }

  return (
    <div className="flex items-center bg-gray-50 border-b border-gray-200 h-9">
      <div className="flex overflow-x-auto overflow-y-hidden flex-1 min-w-0 custom-scrollbar">
        {tabs.map((tab) => (
          <div
            key={tab.id}
            className={`flex items-center gap-1.5 px-3 h-9 border-r border-gray-200 cursor-pointer min-w-[120px] max-w-[200px] transition-colors ${
              tab.isDeleted
                ? "bg-red-50 hover:bg-red-100"
                : tab.id === activeTabId
                ? "bg-white"
                : "bg-gray-100 hover:bg-gray-200"
            }`}
            onClick={() => onTabSelect(tab.id)}
          >
            <span className={`flex-1 overflow-hidden text-ellipsis whitespace-nowrap text-sm ${
              tab.isDeleted ? "text-red-700" : ""
            }`}>
              {getFileName(tab.path, tab.name)}
              {tab.isDeleted && " (deleted)"}
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
    </div>
  );
}
