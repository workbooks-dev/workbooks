import { useState } from 'react';

export default function ClaudeApprovalModal({ pendingChanges, onApprove, onDeny }) {
  const [selectedTools, setSelectedTools] = useState(new Set());

  if (!pendingChanges || pendingChanges.length === 0) {
    return null;
  }

  const toggleTool = (toolName) => {
    const newSet = new Set(selectedTools);
    if (newSet.has(toolName)) {
      newSet.delete(toolName);
    } else {
      newSet.add(toolName);
    }
    setSelectedTools(newSet);
  };

  const selectAll = () => {
    const allTools = new Set(pendingChanges.map(change => change.tool));
    setSelectedTools(allTools);
  };

  const deselectAll = () => {
    setSelectedTools(new Set());
  };

  const handleApprove = () => {
    const allowedTools = Array.from(selectedTools);
    onApprove(allowedTools);
  };

  const toolTypes = [...new Set(pendingChanges.map(c => c.tool))];

  return (
    <div className="fixed inset-0 bg-black/50 flex items-center justify-center z-50">
      <div className="bg-white rounded-lg shadow-xl w-[600px] max-h-[80vh] flex flex-col">
        {/* Header */}
        <div className="px-6 py-4 border-b border-gray-200">
          <h2 className="text-lg font-semibold text-gray-900">Approve Changes</h2>
          <p className="text-sm text-gray-600 mt-1">
            Claude wants to make the following changes. Select which tools to allow:
          </p>
        </div>

        {/* Content - scrollable */}
        <div className="flex-1 overflow-y-auto px-6 py-4">
          <div className="space-y-3">
            {toolTypes.map(tool => {
              const changes = pendingChanges.filter(c => c.tool === tool);
              const isSelected = selectedTools.has(tool);

              return (
                <div key={tool} className="border border-gray-200 rounded-lg overflow-hidden">
                  <button
                    onClick={() => toggleTool(tool)}
                    className={`w-full px-4 py-3 flex items-center justify-between transition-colors ${
                      isSelected ? 'bg-blue-50 border-l-4 border-l-blue-500' : 'bg-gray-50 hover:bg-gray-100'
                    }`}
                  >
                    <div className="flex items-center gap-3">
                      <input
                        type="checkbox"
                        checked={isSelected}
                        onChange={() => {}}
                        className="w-4 h-4 text-blue-600 rounded border-gray-300 focus:ring-blue-500"
                      />
                      <div className="text-left">
                        <div className="font-medium text-gray-900">{tool}</div>
                        <div className="text-sm text-gray-600">
                          {changes.length} action{changes.length !== 1 ? 's' : ''}
                        </div>
                      </div>
                    </div>
                    <div className={`px-2 py-1 rounded text-xs font-medium ${
                      tool === 'Bash' ? 'bg-amber-100 text-amber-800' :
                      tool === 'Edit' ? 'bg-green-100 text-green-800' :
                      tool === 'Write' ? 'bg-purple-100 text-purple-800' :
                      'bg-gray-100 text-gray-800'
                    }`}>
                      {tool}
                    </div>
                  </button>

                  {isSelected && (
                    <div className="px-4 py-3 bg-white border-t border-gray-200">
                      {changes.map((change, idx) => (
                        <div key={change.id} className={idx > 0 ? 'mt-2 pt-2 border-t border-gray-100' : ''}>
                          <p className="text-sm text-gray-700">{change.description}</p>
                          {change.file_path && (
                            <p className="text-xs text-gray-500 mt-1 font-mono">{change.file_path}</p>
                          )}
                        </div>
                      ))}
                    </div>
                  )}
                </div>
              );
            })}
          </div>
        </div>

        {/* Footer */}
        <div className="px-6 py-4 border-t border-gray-200 flex items-center justify-between">
          <div className="flex gap-2">
            <button
              onClick={selectAll}
              className="text-sm text-blue-600 hover:text-blue-700 font-medium"
            >
              Select All
            </button>
            <span className="text-gray-300">|</span>
            <button
              onClick={deselectAll}
              className="text-sm text-gray-600 hover:text-gray-700 font-medium"
            >
              Deselect All
            </button>
          </div>

          <div className="flex gap-3">
            <button
              onClick={onDeny}
              className="px-4 py-2 text-sm font-medium text-gray-700 bg-white border border-gray-300 rounded-lg hover:bg-gray-50 transition-colors"
            >
              Deny All
            </button>
            <button
              onClick={handleApprove}
              disabled={selectedTools.size === 0}
              className={`px-4 py-2 text-sm font-medium rounded-lg transition-colors ${
                selectedTools.size === 0
                  ? 'bg-gray-100 text-gray-400 cursor-not-allowed'
                  : 'bg-blue-600 text-white hover:bg-blue-700'
              }`}
            >
              Approve ({selectedTools.size})
            </button>
          </div>
        </div>
      </div>
    </div>
  );
}
