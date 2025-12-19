import { useState } from "react";

export function SecretsWarningModal({ cellIndices, onClearAndSave, onGoBack, onDangerouslySave }) {
  const [showDangerousConfirm, setShowDangerousConfirm] = useState(false);

  return (
    <div
      className="fixed inset-0 bg-black bg-opacity-50 flex items-center justify-center z-50"
      onClick={onGoBack}
    >
      <div
        className="bg-white rounded-lg shadow-xl max-w-lg w-full p-6"
        onClick={(e) => e.stopPropagation()}
      >
        {/* Header */}
        <div className="flex items-start gap-3 mb-4">
          <div className="flex-shrink-0 w-10 h-10 rounded-full bg-amber-100 flex items-center justify-center text-amber-600 text-xl">
            ⚠
          </div>
          <div className="flex-1">
            <h3 className="text-lg font-semibold text-gray-900 mb-1">
              Secrets Detected in Outputs
            </h3>
            <p className="text-sm text-gray-600 leading-relaxed">
              The following cells contain secret values in their outputs. Saving this workbook may expose sensitive credentials.
            </p>
          </div>
        </div>

        {/* Cell list */}
        <div className="mb-6 px-4 py-3 bg-amber-50 border border-amber-200 rounded-md">
          <p className="text-xs font-medium text-amber-900 mb-2">Affected cells:</p>
          <div className="flex flex-wrap gap-2">
            {cellIndices.map((index) => (
              <span
                key={index}
                className="px-2 py-1 bg-white border border-amber-300 rounded text-xs font-mono text-amber-900"
              >
                Cell [{index + 1}]
              </span>
            ))}
          </div>
        </div>

        {/* Warning message */}
        <div className="mb-6 px-4 py-3 bg-gray-50 border border-gray-200 rounded-md">
          <p className="text-xs text-gray-700 leading-relaxed">
            <strong>Why this matters:</strong> If you save this workbook with secrets in outputs, they could be:
          </p>
          <ul className="mt-2 ml-4 text-xs text-gray-700 space-y-1 list-disc">
            <li>Committed to version control (Git)</li>
            <li>Shared with teammates</li>
            <li>Exposed in backups or logs</li>
          </ul>
        </div>

        {/* Action buttons */}
        {!showDangerousConfirm ? (
          <div className="flex flex-col gap-2">
            <button
              onClick={onClearAndSave}
              className="w-full px-4 py-2.5 text-sm font-medium text-white bg-blue-600 hover:bg-blue-700 rounded-md transition-colors"
            >
              Clear Outputs & Save
            </button>
            <button
              onClick={onGoBack}
              className="w-full px-4 py-2.5 text-sm font-medium text-gray-700 bg-white hover:bg-gray-50 border border-gray-300 rounded-md transition-all"
            >
              Go Back and Fix
            </button>
            <button
              onClick={() => setShowDangerousConfirm(true)}
              className="w-full px-4 py-2.5 text-sm font-medium text-red-700 bg-white hover:bg-red-50 border border-red-300 rounded-md transition-all"
            >
              Save with Secrets (Dangerous)
            </button>
          </div>
        ) : (
          <div className="flex flex-col gap-3">
            <div className="px-4 py-3 bg-red-50 border border-red-200 rounded-md">
              <p className="text-sm font-semibold text-red-900 mb-1">
                ⚠ Are you absolutely sure?
              </p>
              <p className="text-xs text-red-800 leading-relaxed">
                This will save secrets in plain text in the notebook file. Only do this if you understand the security implications and have a specific reason.
              </p>
            </div>
            <div className="flex gap-2">
              <button
                onClick={onDangerouslySave}
                className="flex-1 px-4 py-2.5 text-sm font-medium text-white bg-red-600 hover:bg-red-700 rounded-md transition-colors"
              >
                Yes, Save Anyway
              </button>
              <button
                onClick={() => setShowDangerousConfirm(false)}
                className="flex-1 px-4 py-2.5 text-sm font-medium text-gray-700 bg-white hover:bg-gray-50 border border-gray-300 rounded-md transition-all"
              >
                Cancel
              </button>
            </div>
          </div>
        )}
      </div>
    </div>
  );
}
