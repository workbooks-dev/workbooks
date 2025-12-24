import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";

function ImportEnvDialog({ onConfirm, onCancel, existingKeys }) {
  const [envContent, setEnvContent] = useState("");
  const [parsedSecrets, setParsedSecrets] = useState({ toImport: [], ignored: [] });

  useEffect(() => {
    if (!envContent.trim()) {
      setParsedSecrets({ toImport: [], ignored: [] });
      return;
    }

    const toImport = [];
    const ignored = [];

    for (const line of envContent.split('\n')) {
      const trimmed = line.trim();
      if (!trimmed || trimmed.startsWith('#')) continue;

      const eqIndex = trimmed.indexOf('=');
      if (eqIndex === -1) continue;

      const key = trimmed.substring(0, eqIndex).trim();
      const value = trimmed.substring(eqIndex + 1).trim().replace(/^["']|["']$/g, '');

      if (existingKeys.includes(key)) {
        ignored.push({ key, value });
      } else {
        toImport.push({ key, value });
      }
    }

    setParsedSecrets({ toImport, ignored });
  }, [envContent, existingKeys]);

  const handleImport = () => {
    onConfirm(parsedSecrets.toImport);
  };

  return (
    <div className="fixed inset-0 bg-black bg-opacity-50 flex items-center justify-center z-50" onClick={onCancel}>
      <div className="bg-white rounded-lg shadow-xl max-w-2xl w-full p-6 max-h-[90vh] flex flex-col" onClick={(e) => e.stopPropagation()}>
        <h3 className="text-lg font-semibold text-gray-900 mb-4">Import from .env</h3>

        <div className="mb-4">
          <label className="block text-sm font-medium text-gray-700 mb-2">Paste .env content</label>
          <textarea
            value={envContent}
            onChange={(e) => setEnvContent(e.target.value)}
            placeholder="API_KEY=your-key-here&#10;DATABASE_URL=postgresql://..."
            autoFocus
            className="w-full h-32 px-3 py-2 text-sm font-mono border border-gray-300 rounded-md focus:outline-none focus:ring-2 focus:ring-blue-500 focus:border-transparent resize-none"
          />
          <p className="mt-1 text-xs text-gray-500">Paste your .env file contents above</p>
        </div>

        <div className="flex gap-2">
          <button
            type="button"
            onClick={onCancel}
            className="flex-1 px-4 py-2 text-sm font-medium text-gray-700 bg-white hover:bg-gray-50 border border-gray-300 rounded-md transition-colors"
          >
            Cancel
          </button>
          <button
            type="button"
            onClick={handleImport}
            disabled={parsedSecrets.toImport.length === 0}
            className="flex-1 px-4 py-2 text-sm font-medium text-white bg-blue-600 hover:bg-blue-700 rounded-md transition-colors disabled:opacity-50 disabled:cursor-not-allowed"
          >
            Import {parsedSecrets.toImport.length > 0 ? `${parsedSecrets.toImport.length} Secret${parsedSecrets.toImport.length === 1 ? '' : 's'}` : ''}
          </button>
        </div>

        {parsedSecrets.toImport.length > 0 && (
          <div className="mt-4 p-3 bg-green-50 border border-green-200 rounded-md">
            <p className="text-sm text-green-800 font-medium">
              {parsedSecrets.toImport.length} secret{parsedSecrets.toImport.length === 1 ? '' : 's'} ready to import
            </p>
          </div>
        )}

        {parsedSecrets.ignored.length > 0 && (
          <div className="mt-4 p-3 bg-amber-50 border border-amber-200 rounded-md">
            <h4 className="text-sm font-semibold text-amber-900 mb-2">
              Ignored ({parsedSecrets.ignored.length})
            </h4>
            <p className="text-xs text-amber-700 mb-2">These keys already exist. Edit them directly in the secrets list.</p>
            <div className="flex flex-wrap gap-1">
              {parsedSecrets.ignored.map((secret, idx) => (
                <div key={idx} className="text-xs font-mono text-amber-800 bg-amber-100 px-2 py-1 rounded">
                  {secret.key}
                </div>
              ))}
            </div>
          </div>
        )}
      </div>
    </div>
  );
}

function DeleteSecretDialog({ secretKey, onConfirm, onCancel }) {
  const [isDeleting, setIsDeleting] = useState(false);

  const handleDelete = async () => {
    setIsDeleting(true);
    try {
      await onConfirm();
    } finally {
      setIsDeleting(false);
    }
  };

  return (
    <div className="fixed inset-0 bg-black bg-opacity-50 flex items-center justify-center z-50" onClick={onCancel}>
      <div className="bg-white rounded-lg shadow-xl max-w-md w-full p-6" onClick={(e) => e.stopPropagation()}>
        <h3 className="text-lg font-semibold text-gray-900 mb-4">Delete Secret</h3>
        <p className="text-sm text-gray-700 mb-6">
          Are you sure you want to delete <span className="font-mono font-semibold text-gray-900">{secretKey}</span>?
          <br />
          <span className="text-gray-500 text-xs mt-2 block">This action cannot be undone. Touch ID authentication required.</span>
        </p>
        <div className="flex gap-2">
          <button
            type="button"
            onClick={onCancel}
            className="flex-1 px-4 py-2 text-sm font-medium text-gray-700 bg-white hover:bg-gray-50 border border-gray-300 rounded-md transition-colors"
          >
            Cancel
          </button>
          <button
            type="button"
            onClick={handleDelete}
            disabled={isDeleting}
            className="flex-1 px-4 py-2 text-sm font-medium text-white bg-red-600 hover:bg-red-700 rounded-md transition-colors disabled:opacity-50 disabled:cursor-not-allowed"
          >
            {isDeleting ? "Deleting..." : "Delete"}
          </button>
        </div>
      </div>
    </div>
  );
}

function AddSecretDialog({ onConfirm, onCancel, editSecret = null, projectRoot }) {
  const [key, setKey] = useState(editSecret?.key || "");
  const [value, setValue] = useState("");
  const [originalValue, setOriginalValue] = useState(""); // Track original value for change detection
  const [showValue, setShowValue] = useState(false);
  const [isAuthenticating, setIsAuthenticating] = useState(false);
  const [currentValueLoaded, setCurrentValueLoaded] = useState(false);

  useEffect(() => {
    // If editing, reset state
    if (editSecret) {
      setValue("");
      setOriginalValue("");
      setCurrentValueLoaded(false);
      setShowValue(false);
    }
  }, [editSecret]);

  const handleShowValue = async () => {
    // If editing and haven't loaded the current value yet, authenticate first
    if (editSecret && !currentValueLoaded) {
      setIsAuthenticating(true);
      try {
        // This will trigger Touch ID on macOS
        const currentValue = await invoke("get_secret_authenticated", {
          projectPath: projectRoot,
          key: editSecret.key,
        });
        setValue(currentValue);
        setOriginalValue(currentValue); // Track original for change detection
        setCurrentValueLoaded(true);
        setShowValue(true); // Auto-show after authentication
      } catch (err) {
        // User cancelled or authentication failed - just do nothing
        console.log("Authentication cancelled or failed:", err);
      } finally {
        setIsAuthenticating(false);
      }
    } else {
      // Just toggle visibility
      setShowValue(!showValue);
    }
  };

  // Check if value has actually changed (for Update button)
  const hasValueChanged = editSecret && currentValueLoaded && value !== originalValue;

  const handleSubmit = (e) => {
    e.preventDefault();

    // For new secrets, require a value
    if (!editSecret && !value.trim()) {
      return;
    }

    // For editing, only submit if value has changed
    if (editSecret && !hasValueChanged) {
      return;
    }

    if (key.trim() && value.trim()) {
      onConfirm(key.trim(), value.trim());
    }
  };

  return (
    <div className="fixed inset-0 bg-black bg-opacity-50 flex items-center justify-center z-50" onClick={onCancel}>
      <div className="bg-white rounded-lg shadow-xl max-w-md w-full p-6" onClick={(e) => e.stopPropagation()}>
        <h3 className="text-lg font-semibold text-gray-900 mb-4">{editSecret ? "Edit Secret" : "Add Secret"}</h3>
        <form onSubmit={handleSubmit}>
          <div className="mb-4">
            <label className="block text-sm font-medium text-gray-700 mb-2">Key Name</label>
            <input
              type="text"
              value={key}
              onChange={(e) => setKey(e.target.value)}
              placeholder="e.g., OPENAI_API_KEY"
              readOnly={!!editSecret}
              autoFocus
              className={`w-full px-3 py-2 text-sm border border-gray-300 rounded-md focus:outline-none focus:ring-2 focus:ring-blue-500 focus:border-transparent ${editSecret ? 'bg-gray-50 text-gray-700' : ''}`}
            />
            <p className="mt-1 text-xs text-gray-500">
              {editSecret ? "Key name cannot be changed" : "Use uppercase with underscores (e.g., API_KEY)"}
            </p>
          </div>

          <div className="mb-6">
            <label className="block text-sm font-medium text-gray-700 mb-2">Value</label>
            <div className="relative">
              <input
                type={showValue ? "text" : "password"}
                value={value}
                onChange={(e) => setValue(e.target.value)}
                placeholder={
                  editSecret
                    ? (currentValueLoaded ? "Enter new value or keep current" : "Enter new value to replace current")
                    : "Enter secret value"
                }
                className="w-full px-3 py-2 pr-20 text-sm border border-gray-300 rounded-md focus:outline-none focus:ring-2 focus:ring-blue-500 focus:border-transparent"
              />
              <button
                type="button"
                className="absolute right-2 top-1/2 -translate-y-1/2 px-2 py-1 text-xs font-medium text-gray-600 hover:text-gray-900 transition-colors disabled:opacity-50"
                onClick={handleShowValue}
                disabled={isAuthenticating}
              >
                {isAuthenticating ? "..." : (showValue ? "Hide" : "Show")}
              </button>
            </div>
            {editSecret && !currentValueLoaded && (
              <p className="mt-1 text-xs text-gray-500">
                Click "Show" to view current value (requires Touch ID)
              </p>
            )}
            {editSecret && currentValueLoaded && (
              <p className="mt-1 text-xs text-gray-500">
                Modify the value above or leave unchanged
              </p>
            )}
          </div>

          <div className="flex gap-2">
            <button
              type="submit"
              disabled={editSecret ? !hasValueChanged : (!key.trim() || !value.trim())}
              className="flex-1 px-4 py-2 text-sm font-medium text-white bg-blue-600 hover:bg-blue-700 rounded-md transition-colors disabled:opacity-50 disabled:cursor-not-allowed"
            >
              {editSecret ? "Update" : "Add"}
            </button>
            <button
              type="button"
              onClick={onCancel}
              className="flex-1 px-4 py-2 text-sm font-medium text-gray-700 bg-white hover:bg-gray-50 border border-gray-300 rounded-md transition-colors"
            >
              Cancel
            </button>
          </div>
        </form>
      </div>
    </div>
  );
}

export function SecretsManager({ projectRoot, onClose }) {
  const [secrets, setSecrets] = useState([]);
  const [loading, setLoading] = useState(true);
  const [showAddDialog, setShowAddDialog] = useState(false);
  const [editingSecret, setEditingSecret] = useState(null);
  const [deletingSecret, setDeletingSecret] = useState(null);
  const [showImportDialog, setShowImportDialog] = useState(false);
  const [searchTerm, setSearchTerm] = useState("");
  const [error, setError] = useState(null);

  useEffect(() => {
    loadSecrets();
  }, [projectRoot]);

  const loadSecrets = async () => {
    setLoading(true);
    setError(null);
    try {
      const secretsList = await invoke("list_secrets", {
        projectPath: projectRoot,
      });
      setSecrets(secretsList);
    } catch (err) {
      console.error("Failed to load secrets:", err);
      setError(err.toString());
    } finally {
      setLoading(false);
    }
  };

  const handleAddSecret = async (key, value) => {
    try {
      await invoke("add_secret", {
        projectPath: projectRoot,
        key,
        value,
      });
      setShowAddDialog(false);
      await loadSecrets();
      // Notify sidebar to update secrets count
      window.dispatchEvent(new CustomEvent("workbooks:secrets-changed"));
    } catch (err) {
      console.error("Failed to add secret:", err);
      alert(`Failed to add secret: ${err}`);
    }
  };

  const handleUpdateSecret = async (key, value) => {
    try {
      await invoke("update_secret", {
        projectPath: projectRoot,
        key,
        value,
      });
      setEditingSecret(null);
      await loadSecrets();
      // Notify sidebar to update secrets count
      window.dispatchEvent(new CustomEvent("workbooks:secrets-changed"));
    } catch (err) {
      // User cancelled authentication or other error - just log it, don't alert
      console.log("Update cancelled or failed:", err);
      // Don't close the dialog so user can try again or cancel manually
    }
  };

  const handleDeleteSecret = (secret) => {
    setDeletingSecret(secret);
  };

  const confirmDeleteSecret = async () => {
    if (!deletingSecret) return;

    try {
      await invoke("delete_secret", {
        projectPath: projectRoot,
        key: deletingSecret.key,
      });
      setDeletingSecret(null);
      await loadSecrets();
      // Notify sidebar to update secrets count
      window.dispatchEvent(new CustomEvent("workbooks:secrets-changed"));
    } catch (err) {
      // User cancelled authentication or other error - just log it, don't alert
      console.log("Delete cancelled or failed:", err);
      // Keep modal open so user can try again or cancel manually
    }
  };

  const handleImportSecrets = async (secretsToImport) => {
    try {
      // Import each secret
      for (const { key, value } of secretsToImport) {
        await invoke("add_secret", {
          projectPath: projectRoot,
          key,
          value,
        });
      }

      setShowImportDialog(false);
      await loadSecrets();
      // Notify sidebar to update secrets count
      window.dispatchEvent(new CustomEvent("workbooks:secrets-changed"));
    } catch (err) {
      console.error("Failed to import secrets:", err);
      alert(`Failed to import secrets: ${err}`);
    }
  };

  const filteredSecrets = secrets.filter((secret) =>
    secret.key.toLowerCase().includes(searchTerm.toLowerCase())
  );

  const formatDate = (timestamp) => {
    return new Date(timestamp * 1000).toLocaleString();
  };

  return (
    <div className="flex flex-col h-full bg-white">
      <div className="flex justify-between items-center px-6 py-4 border-b border-gray-200">
        <div>
          <h2 className="text-lg font-semibold text-gray-900">Secrets Manager</h2>
          <p className="text-sm text-gray-600 mt-0.5">Securely store API keys and credentials</p>
        </div>
        <button
          className="text-gray-400 hover:text-gray-600 text-2xl px-2 transition-colors"
          onClick={onClose}
        >
          ✕
        </button>
      </div>

      <div className="flex gap-3 px-6 py-4 border-b border-gray-200">
        <input
          type="text"
          className="flex-1 px-3 py-2 text-sm border border-gray-300 rounded-md focus:outline-none focus:ring-2 focus:ring-blue-500 focus:border-transparent"
          placeholder="Search secrets..."
          value={searchTerm}
          onChange={(e) => setSearchTerm(e.target.value)}
        />
        <div className="flex gap-2">
          <button
            className="px-3 py-2 text-sm font-medium text-gray-700 bg-white hover:bg-gray-50 border border-gray-300 rounded-md transition-colors"
            onClick={() => setShowImportDialog(true)}
          >
            Import .env
          </button>
          <button
            className="px-3 py-2 text-sm font-medium text-white bg-blue-600 hover:bg-blue-700 rounded-md transition-colors"
            onClick={() => setShowAddDialog(true)}
          >
            + Add Secret
          </button>
        </div>
      </div>

      {error && (
        <div className="mx-6 mt-4 bg-red-50 border border-red-200 rounded-lg px-4 py-3">
          <p className="text-sm text-red-800">Failed to load secrets: {error}</p>
        </div>
      )}

      {loading ? (
        <div className="flex-1 flex items-center justify-center">
          <p className="text-gray-500">Loading secrets...</p>
        </div>
      ) : filteredSecrets.length === 0 ? (
        <div className="flex-1 flex flex-col items-center justify-center py-12 text-center">
          <div className="text-5xl mb-4">🔐</div>
          <h3 className="text-base font-semibold text-gray-900 mb-2">No secrets yet</h3>
          <p className="text-sm text-gray-600 mb-6">
            {searchTerm
              ? "No secrets match your search."
              : "Add your first secret to get started."}
          </p>
          {!searchTerm && (
            <button
              className="px-4 py-2 text-sm font-medium text-white bg-blue-600 hover:bg-blue-700 rounded-md transition-colors"
              onClick={() => setShowAddDialog(true)}
            >
              + Add Secret
            </button>
          )}
        </div>
      ) : (
        <div className="flex-1 overflow-y-auto px-6 py-4">
          <table className="w-full border-collapse">
            <thead>
              <tr>
                <th className="text-left px-4 py-3 bg-gray-50 text-gray-700 text-xs font-semibold uppercase tracking-wider border-b border-gray-200">
                  Key
                </th>
                <th className="text-left px-4 py-3 bg-gray-50 text-gray-700 text-xs font-semibold uppercase tracking-wider border-b border-gray-200">
                  Created
                </th>
                <th className="text-left px-4 py-3 bg-gray-50 text-gray-700 text-xs font-semibold uppercase tracking-wider border-b border-gray-200">
                  Last Modified
                </th>
                <th className="text-left px-4 py-3 bg-gray-50 text-gray-700 text-xs font-semibold uppercase tracking-wider border-b border-gray-200">
                  Actions
                </th>
              </tr>
            </thead>
            <tbody>
              {filteredSecrets.map((secret) => (
                <tr key={secret.id} className="hover:bg-gray-50">
                  <td className="px-4 py-3 border-b border-gray-200">
                    <code className="bg-gray-100 px-2 py-1 rounded text-sm font-mono text-gray-900">
                      {secret.key}
                    </code>
                  </td>
                  <td className="px-4 py-3 border-b border-gray-200 text-sm text-gray-600">
                    {formatDate(secret.created_at)}
                  </td>
                  <td className="px-4 py-3 border-b border-gray-200 text-sm text-gray-600">
                    {formatDate(secret.modified_at)}
                  </td>
                  <td className="px-4 py-3 border-b border-gray-200">
                    <div className="flex gap-2">
                      <button
                        className="px-2 py-1 text-sm text-gray-600 hover:text-gray-900 hover:bg-gray-100 rounded transition-colors"
                        onClick={() => setEditingSecret(secret)}
                        title="Edit secret"
                      >
                        Edit
                      </button>
                      <button
                        className="px-2 py-1 text-sm text-red-600 hover:text-red-700 hover:bg-red-50 rounded transition-colors"
                        onClick={() => handleDeleteSecret(secret)}
                        title="Delete secret"
                      >
                        Delete
                      </button>
                    </div>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}

      {showAddDialog && (
        <AddSecretDialog
          projectRoot={projectRoot}
          onConfirm={handleAddSecret}
          onCancel={() => setShowAddDialog(false)}
        />
      )}

      {editingSecret && (
        <AddSecretDialog
          projectRoot={projectRoot}
          editSecret={editingSecret}
          onConfirm={(key, value) => handleUpdateSecret(key, value)}
          onCancel={() => setEditingSecret(null)}
        />
      )}

      {deletingSecret && (
        <DeleteSecretDialog
          secretKey={deletingSecret.key}
          onConfirm={confirmDeleteSecret}
          onCancel={() => setDeletingSecret(null)}
        />
      )}

      {showImportDialog && (
        <ImportEnvDialog
          existingKeys={secrets.map(s => s.key)}
          onConfirm={handleImportSecrets}
          onCancel={() => setShowImportDialog(false)}
        />
      )}
    </div>
  );
}
