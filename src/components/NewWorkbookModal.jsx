import { useState, useEffect, useRef } from "react";

export default function NewWorkbookModal({
  isOpen,
  onClose,
  onCreateBlank,
  onGenerateWithAI,
}) {
  const [workbookName, setWorkbookName] = useState("");
  const [description, setDescription] = useState("");
  const nameInputRef = useRef(null);

  useEffect(() => {
    if (isOpen && nameInputRef.current) {
      nameInputRef.current.focus();
    }
  }, [isOpen]);

  useEffect(() => {
    const handleEscape = (e) => {
      if (e.key === "Escape" && isOpen) {
        onClose();
      }
    };

    document.addEventListener("keydown", handleEscape);
    return () => document.removeEventListener("keydown", handleEscape);
  }, [isOpen, onClose]);

  const handleCreateBlank = () => {
    const name = workbookName.trim() || "Untitled";
    onCreateBlank(name);
    setWorkbookName("");
    setDescription("");
  };

  const handleGenerateWithAI = () => {
    const name = workbookName.trim() || "Untitled";
    if (description.trim()) {
      onGenerateWithAI(name, description.trim());
      setWorkbookName("");
      setDescription("");
    }
  };

  const handleSubmit = (e) => {
    e.preventDefault();
    if (description.trim()) {
      handleGenerateWithAI();
    } else {
      handleCreateBlank();
    }
  };

  if (!isOpen) return null;

  return (
    <div className="fixed inset-0 bg-black bg-opacity-50 flex items-center justify-center z-50">
      <div className="bg-white rounded-lg shadow-xl w-full max-w-2xl mx-4">
        {/* Header */}
        <div className="px-6 py-4 border-b border-gray-200">
          <h2 className="text-lg font-semibold text-gray-900">
            Create New Workbook
          </h2>
        </div>

        {/* Content */}
        <form onSubmit={handleSubmit} className="p-6">
          <div className="mb-4">
            <label
              htmlFor="workbook-name"
              className="block text-sm font-medium text-gray-700 mb-2"
            >
              Workbook Name
            </label>
            <input
              ref={nameInputRef}
              id="workbook-name"
              type="text"
              value={workbookName}
              onChange={(e) => setWorkbookName(e.target.value)}
              placeholder="My Workbook (optional)"
              className="w-full px-3 py-2 text-sm border border-gray-300 rounded focus:outline-none focus:ring-2 focus:ring-blue-500"
            />
          </div>

          <div className="mb-4">
            <label
              htmlFor="workbook-description"
              className="block text-sm font-medium text-gray-700 mb-2"
            >
              What do you want to build?
            </label>
            <textarea
              id="workbook-description"
              value={description}
              onChange={(e) => setDescription(e.target.value)}
              placeholder="Describe what you want to create, or leave blank for an empty workbook..."
              className="w-full px-3 py-2 text-sm border border-gray-300 rounded focus:outline-none focus:ring-2 focus:ring-blue-500 resize-none"
              rows={6}
            />
          </div>

          {/* Footer */}
          <div className="flex items-center justify-between gap-3 pt-4 border-t border-gray-200">
            <button
              type="button"
              onClick={onClose}
              className="px-4 py-2 text-sm font-medium text-gray-700 bg-white border border-gray-300 rounded hover:bg-gray-50 transition-colors"
            >
              Cancel
            </button>

            <div className="flex items-center gap-3">
              <button
                type="button"
                onClick={handleCreateBlank}
                className="px-4 py-2 text-sm font-medium text-gray-700 bg-white border border-gray-300 rounded hover:bg-gray-50 transition-colors"
              >
                Create Blank Notebook
              </button>
              <button
                type="submit"
                disabled={!description.trim()}
                className="px-4 py-2 text-sm font-medium text-white bg-blue-600 rounded hover:bg-blue-700 transition-colors disabled:opacity-50 disabled:cursor-not-allowed"
              >
                Generate with AI
              </button>
            </div>
          </div>
        </form>
      </div>
    </div>
  );
}
