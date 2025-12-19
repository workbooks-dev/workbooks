import { useEffect, useRef } from "react";

/**
 * SaveConfirmDialog - A modal dialog for confirming unsaved changes
 * Provides three options: Save, Don't Save, Cancel
 */
export function SaveConfirmDialog({ isOpen, onSave, onDontSave, onCancel, message }) {
  const dialogRef = useRef(null);

  useEffect(() => {
    if (isOpen) {
      // Focus the Save button when dialog opens
      dialogRef.current?.querySelector('[data-action="save"]')?.focus();

      // Handle Escape key
      const handleEscape = (e) => {
        if (e.key === "Escape") {
          onCancel();
        }
      };

      window.addEventListener("keydown", handleEscape);
      return () => window.removeEventListener("keydown", handleEscape);
    }
  }, [isOpen, onCancel]);

  if (!isOpen) return null;

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center">
      {/* Backdrop */}
      <div
        className="absolute inset-0 bg-black bg-opacity-50"
        onClick={onCancel}
      />

      {/* Dialog */}
      <div
        ref={dialogRef}
        className="relative bg-white rounded-lg shadow-xl max-w-md w-full mx-4 p-6"
        role="dialog"
        aria-modal="true"
        aria-labelledby="dialog-title"
      >
        <div className="mb-4">
          <h3
            id="dialog-title"
            className="text-lg font-semibold text-gray-900 mb-2"
          >
            Unsaved Changes
          </h3>
          <p className="text-sm text-gray-600">
            {message || "You have unsaved changes. What would you like to do?"}
          </p>
        </div>

        <div className="flex gap-3 justify-end">
          <button
            onClick={onCancel}
            className="px-4 py-2 text-sm font-medium text-gray-700 bg-white border border-gray-300 rounded-md hover:bg-gray-50 focus:outline-none focus:ring-2 focus:ring-offset-2 focus:ring-blue-500"
          >
            Cancel
          </button>
          <button
            onClick={onDontSave}
            className="px-4 py-2 text-sm font-medium text-white bg-red-600 rounded-md hover:bg-red-700 focus:outline-none focus:ring-2 focus:ring-offset-2 focus:ring-red-500"
          >
            Don't Save
          </button>
          <button
            data-action="save"
            onClick={onSave}
            className="px-4 py-2 text-sm font-medium text-white bg-blue-600 rounded-md hover:bg-blue-700 focus:outline-none focus:ring-2 focus:ring-offset-2 focus:ring-blue-500"
          >
            Save
          </button>
        </div>
      </div>
    </div>
  );
}
