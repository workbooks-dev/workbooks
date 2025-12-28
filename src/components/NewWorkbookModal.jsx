import { useState, useEffect, useRef } from "react";

export default function NewWorkbookModal({
  isOpen,
  onClose,
  onCreateBlank,
  onGenerateWithAI,
  isGenerating = false,
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
      if (e.key === "Escape" && isOpen && !isGenerating) {
        onClose();
      }
    };

    document.addEventListener("keydown", handleEscape);
    return () => document.removeEventListener("keydown", handleEscape);
  }, [isOpen, isGenerating, onClose]);

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

  // Prevent keyboard events from propagating to the notebook below
  const handleKeyDown = (e) => {
    e.stopPropagation();
  };

  return (
    <div
      className="fixed inset-0 bg-black bg-opacity-50 flex items-center justify-center z-50"
      onKeyDown={handleKeyDown}
    >
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
              disabled={isGenerating}
              className="px-4 py-2 text-sm font-medium text-gray-700 bg-white border border-gray-300 rounded hover:bg-gray-50 transition-colors disabled:opacity-50 disabled:cursor-not-allowed"
            >
              Cancel
            </button>

            <div className="flex items-center gap-3">
              <button
                type="button"
                onClick={handleCreateBlank}
                disabled={isGenerating}
                className="px-4 py-2 text-sm font-medium text-gray-700 bg-white border border-gray-300 rounded hover:bg-gray-50 transition-colors disabled:opacity-50 disabled:cursor-not-allowed"
              >
                Create Blank Notebook
              </button>
              <button
                type="submit"
                disabled={!description.trim() || isGenerating}
                className="px-4 py-2 text-sm font-medium text-white bg-blue-600 rounded hover:bg-blue-700 transition-colors disabled:opacity-50 disabled:cursor-not-allowed flex items-center gap-2"
              >
                {isGenerating && (
                  <svg className="animate-spin h-4 w-4" xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24">
                    <circle className="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" strokeWidth="4"></circle>
                    <path className="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z"></path>
                  </svg>
                )}
                {isGenerating ? "Generating..." : "Generate with AI"}
              </button>
            </div>
          </div>
        </form>
      </div>
    </div>
  );
}
