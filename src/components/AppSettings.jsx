import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";

export function AppSettings({ onClose }) {
  const [apiKey, setApiKey] = useState("");
  const [hasKey, setHasKey] = useState(false);
  const [saving, setSaving] = useState(false);
  const [loading, setLoading] = useState(true);
  const [message, setMessage] = useState(null);
  const [aiEnabled, setAiEnabled] = useState(false);

  useEffect(() => {
    loadSettings();
  }, []);

  const detectPlatform = () => {
    const userAgent = window.navigator.userAgent;
    const platform = window.navigator.platform;

    if (platform.indexOf('Mac') !== -1 || userAgent.indexOf('Mac') !== -1) {
      return 'Darwin';
    } else if (platform.indexOf('Win') !== -1 || userAgent.indexOf('Windows') !== -1) {
      return 'Windows_NT';
    } else if (platform.indexOf('Linux') !== -1 || userAgent.indexOf('Linux') !== -1) {
      return 'Linux';
    }
    return 'Unknown';
  };

  const getKeychainName = () => {
    const platform = detectPlatform();
    switch (platform) {
      case "Darwin":
        return "macOS Keychain";
      case "Windows_NT":
        return "Windows Credential Manager";
      case "Linux":
        return "system keychain";
      default:
        return "system keychain";
    }
  };

  const loadSettings = async () => {
    try {
      setLoading(true);

      // Check if API key exists
      const keyExists = await invoke("check_anthropic_api_key");
      setHasKey(keyExists);

      // Load global config
      const config = await invoke("get_global_config");
      setAiEnabled(config.ai?.enabled || false);
    } catch (err) {
      console.error("Failed to load settings:", err);
      showMessage("Failed to load settings", "error");
    } finally {
      setLoading(false);
    }
  };

  const showMessage = (text, type = "success") => {
    setMessage({ text, type });
    setTimeout(() => setMessage(null), 3000);
  };

  const handleSave = async () => {
    if (!apiKey.trim()) {
      showMessage("API key cannot be empty", "error");
      return;
    }

    setSaving(true);
    try {
      await invoke("save_anthropic_api_key", { key: apiKey.trim() });
      await invoke("set_ai_features_enabled", { enabled: true });

      setHasKey(true);
      setAiEnabled(true);
      setApiKey(""); // Clear input after saving
      showMessage("API key saved securely in system keychain");
    } catch (err) {
      console.error("Failed to save API key:", err);
      showMessage(err.toString(), "error");
    } finally {
      setSaving(false);
    }
  };

  const handleRemove = async () => {
    if (!confirm("Remove API key from keychain?")) {
      return;
    }

    try {
      await invoke("remove_anthropic_api_key");
      await invoke("set_ai_features_enabled", { enabled: false });

      setHasKey(false);
      setAiEnabled(false);
      setApiKey("");
      showMessage("API key removed");
    } catch (err) {
      console.error("Failed to remove API key:", err);
      showMessage(err.toString(), "error");
    }
  };

  const handleToggleAI = async (enabled) => {
    try {
      await invoke("set_ai_features_enabled", { enabled });
      setAiEnabled(enabled);
      showMessage(enabled ? "AI features enabled" : "AI features disabled");
    } catch (err) {
      console.error("Failed to toggle AI features:", err);
      showMessage(err.toString(), "error");
    }
  };

  if (loading) {
    return (
      <div className="h-full flex items-center justify-center">
        <div className="text-gray-500">Loading settings...</div>
      </div>
    );
  }

  return (
    <div className="h-full overflow-y-auto bg-gray-50">
      <div className="max-w-4xl mx-auto p-8">
        {/* Header with Back Button */}
        <div className="mb-8">
          {onClose && (
            <button
              onClick={onClose}
              className="mb-4 inline-flex items-center gap-2 text-sm text-gray-600 hover:text-gray-900 transition-colors"
            >
              <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M15 19l-7-7 7-7" />
              </svg>
              Back
            </button>
          )}
          <h1 className="text-3xl font-bold text-gray-900">Settings</h1>
          <p className="mt-2 text-sm text-gray-600">
            Configure app-wide settings and preferences
          </p>
        </div>

        {/* Message Banner */}
        {message && (
          <div
            className={`mb-6 px-4 py-3 rounded-lg border ${
              message.type === "error"
                ? "bg-red-50 border-red-200 text-red-800"
                : "bg-green-50 border-green-200 text-green-800"
            }`}
          >
            {message.text}
          </div>
        )}

        {/* AI Assistant Section */}
        <div className="bg-white rounded-lg shadow-sm border border-gray-200 overflow-hidden mb-6">
          <div className="px-6 py-4 border-b border-gray-200">
            <h2 className="text-lg font-semibold text-gray-900">AI Assistant</h2>
            <p className="mt-1 text-sm text-gray-600">
              Configure AI-powered features using Claude Agent SDK
            </p>
          </div>

          <div className="px-6 py-4 space-y-6">
            {/* Enable/Disable Toggle */}
            <div className="flex items-center justify-between">
              <div>
                <h3 className="text-base font-medium text-gray-900">
                  Enable AI Features
                </h3>
                <p className="mt-1 text-sm text-gray-500">
                  AI-powered automation and assistance
                </p>
              </div>
              <button
                onClick={() => handleToggleAI(!aiEnabled)}
                disabled={!hasKey}
                className={`relative inline-flex h-6 w-11 items-center rounded-full transition-colors disabled:opacity-50 disabled:cursor-not-allowed ${
                  aiEnabled ? "bg-blue-600" : "bg-gray-200"
                }`}
              >
                <span
                  className={`inline-block h-4 w-4 transform rounded-full bg-white transition-transform ${
                    aiEnabled ? "translate-x-6" : "translate-x-1"
                  }`}
                />
              </button>
            </div>

            {/* API Key Section */}
            <div className="pt-4 border-t border-gray-200">
              <label className="block text-sm font-medium text-gray-900 mb-2">
                Anthropic API Key
              </label>

              {hasKey ? (
                <div className="flex items-center gap-3">
                  <div className="flex-1 px-4 py-2.5 bg-gray-50 rounded-lg border border-gray-200 text-gray-500 font-mono text-sm">
                    ••••••••••••••••••••
                  </div>
                  <button
                    onClick={handleRemove}
                    className="px-4 py-2.5 border border-red-200 text-red-600 rounded-lg hover:bg-red-50 transition-colors text-sm font-medium"
                  >
                    Remove
                  </button>
                </div>
              ) : (
                <div className="flex items-center gap-3">
                  <input
                    type="password"
                    value={apiKey}
                    onChange={(e) => setApiKey(e.target.value)}
                    placeholder="sk-ant-..."
                    className="flex-1 px-4 py-2.5 border border-gray-300 rounded-lg focus:outline-none focus:ring-2 focus:ring-blue-500 focus:border-transparent text-sm"
                    disabled={saving}
                  />
                  <button
                    onClick={handleSave}
                    disabled={!apiKey || saving}
                    className="px-6 py-2.5 bg-blue-600 text-white rounded-lg hover:bg-blue-700 disabled:opacity-50 disabled:cursor-not-allowed transition-colors text-sm font-medium"
                  >
                    {saving ? "Saving..." : "Save"}
                  </button>
                </div>
              )}

              <div className="mt-3 space-y-1.5 text-xs text-gray-500">
                <p>✓ Stored securely in {getKeychainName()}</p>
                <p>✓ Protected by OS-level encryption</p>
                <p>
                  Get your API key from{" "}
                  <a
                    href="https://console.anthropic.com"
                    target="_blank"
                    rel="noopener noreferrer"
                    className="text-blue-600 hover:text-blue-700 underline"
                  >
                    console.anthropic.com
                  </a>
                </p>
              </div>
            </div>
          </div>
        </div>
      </div>
    </div>
  );
}
