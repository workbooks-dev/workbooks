import { useState } from "react";

/**
 * NotebookDiffModal - Shows cell-by-cell changes when AI modifies a notebook
 * Similar to Cursor/Windsurf/Antigravity change approval UI
 */
export function NotebookDiffModal({ oldNotebook, newNotebook, notebookPath, onApprove, onReject, onClose }) {
  const [loading, setLoading] = useState(false);

  // Calculate diff between old and new notebooks
  const calculateDiff = () => {
    const changes = [];
    const oldCells = oldNotebook?.cells || [];
    const newCells = newNotebook?.cells || [];

    // Track which old cells have been matched
    const matchedOldCells = new Set();

    // First pass: find exact matches and modifications
    newCells.forEach((newCell, newIdx) => {
      const oldIdx = oldCells.findIndex((oldCell, idx) =>
        !matchedOldCells.has(idx) &&
        oldCell.cell_type === newCell.cell_type &&
        JSON.stringify(oldCell.source) === JSON.stringify(newCell.source)
      );

      if (oldIdx !== -1) {
        // Exact match - no change
        matchedOldCells.add(oldIdx);
        changes.push({
          type: "unchanged",
          oldIndex: oldIdx,
          newIndex: newIdx,
          cell: newCell,
        });
      } else {
        // Check if this is a modification of an existing cell
        const modifiedIdx = oldCells.findIndex((oldCell, idx) =>
          !matchedOldCells.has(idx) &&
          oldCell.cell_type === newCell.cell_type
        );

        if (modifiedIdx !== -1) {
          matchedOldCells.add(modifiedIdx);
          changes.push({
            type: "modified",
            oldIndex: modifiedIdx,
            newIndex: newIdx,
            oldCell: oldCells[modifiedIdx],
            newCell: newCell,
          });
        } else {
          // New cell added
          changes.push({
            type: "added",
            newIndex: newIdx,
            cell: newCell,
          });
        }
      }
    });

    // Second pass: find deleted cells
    oldCells.forEach((oldCell, oldIdx) => {
      if (!matchedOldCells.has(oldIdx)) {
        changes.push({
          type: "deleted",
          oldIndex: oldIdx,
          cell: oldCell,
        });
      }
    });

    return changes;
  };

  const changes = calculateDiff();

  // Count changes by type
  const summary = changes.reduce(
    (acc, change) => {
      if (change.type === "added") acc.added++;
      else if (change.type === "modified") acc.modified++;
      else if (change.type === "deleted") acc.deleted++;
      return acc;
    },
    { added: 0, modified: 0, deleted: 0 }
  );

  const handleApprove = async () => {
    setLoading(true);
    try {
      await onApprove();
    } finally {
      setLoading(false);
    }
  };

  const handleReject = async () => {
    setLoading(true);
    try {
      await onReject();
    } finally {
      setLoading(false);
    }
  };

  // Get cell source as string
  const getCellSource = (cell) => {
    if (Array.isArray(cell.source)) {
      return cell.source.join("");
    }
    return cell.source || "";
  };

  // Get cell type label
  const getCellTypeLabel = (cellType) => {
    if (cellType === "code") return "Code";
    if (cellType === "markdown") return "Markdown";
    return cellType;
  };

  return (
    <div
      className="fixed inset-0 bg-black bg-opacity-50 flex items-center justify-center z-50 p-4"
      onClick={onClose}
    >
      <div
        className="bg-white rounded-lg shadow-xl w-full max-w-5xl max-h-[90vh] flex flex-col"
        onClick={(e) => e.stopPropagation()}
      >
        {/* Header */}
        <div className="px-6 py-4 border-b border-gray-200 flex items-center justify-between flex-shrink-0">
          <div>
            <h3 className="text-lg font-semibold text-gray-900">Review Changes</h3>
            <p className="text-sm text-gray-600 mt-1">
              Claude wants to modify <span className="font-mono font-medium">{notebookPath.split('/').pop()}</span>
            </p>
          </div>
          <button
            onClick={onClose}
            className="p-1 hover:bg-gray-100 rounded transition-colors"
            title="Close"
          >
            <svg className="w-5 h-5 text-gray-600" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M6 18L18 6M6 6l12 12" />
            </svg>
          </button>
        </div>

        {/* Summary */}
        <div className="px-6 py-3 bg-gray-50 border-b border-gray-200 flex items-center gap-4 flex-shrink-0">
          <span className="text-sm text-gray-700 font-medium">Changes:</span>
          {summary.added > 0 && (
            <span className="text-xs px-2 py-1 rounded-md font-medium bg-emerald-50 text-emerald-700">
              +{summary.added} added
            </span>
          )}
          {summary.modified > 0 && (
            <span className="text-xs px-2 py-1 rounded-md font-medium bg-blue-50 text-blue-700">
              ~{summary.modified} modified
            </span>
          )}
          {summary.deleted > 0 && (
            <span className="text-xs px-2 py-1 rounded-md font-medium bg-red-50 text-red-700">
              -{summary.deleted} deleted
            </span>
          )}
          {summary.added === 0 && summary.modified === 0 && summary.deleted === 0 && (
            <span className="text-xs px-2 py-1 rounded-md font-medium bg-gray-100 text-gray-600">
              No changes
            </span>
          )}
        </div>

        {/* Diff Content */}
        <div className="flex-1 overflow-y-auto px-6 py-4">
          <div className="space-y-4">
            {changes.map((change, idx) => (
              <div
                key={idx}
                className={`border rounded-lg overflow-hidden ${
                  change.type === "added"
                    ? "border-emerald-200 bg-emerald-50"
                    : change.type === "deleted"
                    ? "border-red-200 bg-red-50"
                    : change.type === "modified"
                    ? "border-blue-200 bg-blue-50"
                    : "border-gray-200 bg-white"
                }`}
              >
                {/* Change header */}
                <div className={`px-3 py-2 flex items-center justify-between ${
                  change.type === "added"
                    ? "bg-emerald-100"
                    : change.type === "deleted"
                    ? "bg-red-100"
                    : change.type === "modified"
                    ? "bg-blue-100"
                    : "bg-gray-100"
                }`}>
                  <div className="flex items-center gap-2">
                    <span className={`text-xs font-semibold uppercase tracking-wide ${
                      change.type === "added"
                        ? "text-emerald-700"
                        : change.type === "deleted"
                        ? "text-red-700"
                        : change.type === "modified"
                        ? "text-blue-700"
                        : "text-gray-700"
                    }`}>
                      {change.type}
                    </span>
                    <span className="text-xs text-gray-600">
                      {getCellTypeLabel(change.cell?.cell_type || change.newCell?.cell_type || change.oldCell?.cell_type)}
                    </span>
                  </div>
                  {change.newIndex !== undefined && (
                    <span className="text-xs text-gray-500 font-mono">
                      Cell {change.newIndex + 1}
                    </span>
                  )}
                </div>

                {/* Cell content */}
                {change.type === "modified" ? (
                  <div className="grid grid-cols-2 divide-x divide-gray-200">
                    {/* Old content */}
                    <div className="p-3 bg-red-50">
                      <div className="text-xs text-red-700 font-medium mb-2">Before:</div>
                      <pre className="text-xs font-mono text-gray-800 whitespace-pre-wrap overflow-x-auto">
                        {getCellSource(change.oldCell)}
                      </pre>
                    </div>
                    {/* New content */}
                    <div className="p-3 bg-emerald-50">
                      <div className="text-xs text-emerald-700 font-medium mb-2">After:</div>
                      <pre className="text-xs font-mono text-gray-800 whitespace-pre-wrap overflow-x-auto">
                        {getCellSource(change.newCell)}
                      </pre>
                    </div>
                  </div>
                ) : (
                  <div className="p-3">
                    <pre className="text-xs font-mono text-gray-800 whitespace-pre-wrap overflow-x-auto">
                      {getCellSource(change.cell || change.newCell || change.oldCell)}
                    </pre>
                  </div>
                )}
              </div>
            ))}

            {changes.length === 0 && (
              <div className="text-center py-12 text-gray-500">
                <p className="text-sm">No changes detected</p>
              </div>
            )}
          </div>
        </div>

        {/* Footer Actions */}
        <div className="px-6 py-4 border-t border-gray-200 flex items-center justify-end gap-3 flex-shrink-0">
          <button
            onClick={handleReject}
            disabled={loading}
            className="px-4 py-2 text-sm font-medium text-red-700 bg-white hover:bg-red-50 border border-red-300 rounded-md transition-all disabled:opacity-50 disabled:cursor-not-allowed"
          >
            Reject Changes
          </button>
          <button
            onClick={handleApprove}
            disabled={loading}
            className="px-4 py-2 text-sm font-medium text-white bg-blue-600 hover:bg-blue-700 rounded-md transition-colors shadow-sm disabled:opacity-50 disabled:cursor-not-allowed"
          >
            {loading ? "Applying..." : "Approve & Apply"}
          </button>
        </div>
      </div>
    </div>
  );
}
