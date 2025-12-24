import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";

export function AppSettings({ onClose }) {
  const [apiKey, setApiKey] = useState("");
  const [hasKey, setHasKey] = useState(false);
  const [saving, setSaving] = useState(false);
  const [loading, setLoading] = useState(true);
  const [message, setMessage] = useState(null);
  const [aiEnabled, setAiEnabled] = useState(false);
  const [showValue, setShowValue] = useState(false);
  const [isAuthenticating, setIsAuthenticating] = useState(false);
  const [loadedKeyValue, setLoadedKeyValue] = useState("");
  const [validationStatus, setValidationStatus] = useState(null); // null, "valid", "invalid", "verifying"
  const [validationMessage, setValidationMessage] = useState("");

  useEffect(() => {
    loadSettings();
  }, []);


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

  const validateFormat = (key) => {
    if (!key) {
      setValidationStatus(null);
      setValidationMessage("");
      return false;
    }

    if (!key.startsWith("sk-ant-")) {
      setValidationStatus("invalid");
      setValidationMessage("Key should start with 'sk-ant-'");
      return false;
    }

    if (key.length < 20) {
      setValidationStatus("invalid");
      setValidationMessage("Key appears too short");
      return false;
    }

    setValidationStatus("valid");
    setValidationMessage("Format looks good");
    return true;
  };

  const handleApiKeyChange = (e) => {
    const value = e.target.value;
    setApiKey(value);
    validateFormat(value);
  };

  const handleSave = async () => {
    if (!apiKey.trim()) {
      showMessage("API key cannot be empty", "error");
      return;
    }

    // Format validation first
    if (!validateFormat(apiKey.trim())) {
      showMessage(validationMessage, "error");
      return;
    }

    setSaving(true);
    setValidationStatus("verifying");
    setValidationMessage("Verifying with Anthropic API...");

    try {
      // Verify the API key works
      await invoke("verify_anthropic_api_key", { key: apiKey.trim() });
      setValidationStatus("valid");
      setValidationMessage("API key verified successfully!");

      // Save encrypted
      await invoke("save_anthropic_api_key", { key: apiKey.trim() });
      await invoke("set_ai_features_enabled", { enabled: true });

      setHasKey(true);
      setAiEnabled(true);
      setApiKey(""); // Clear input after saving
      setValidationStatus(null);
      setValidationMessage("");

      showMessage("API key saved securely");
    } catch (err) {
      console.error("Failed to save API key:", err);
      setValidationStatus("invalid");
      setValidationMessage(err.toString());
      showMessage(err.toString(), "error");
    } finally {
      setSaving(false);
    }
  };

  const handleRemove = async () => {
    if (!confirm("Remove API key?")) {
      return;
    }

    try {
      await invoke("remove_anthropic_api_key");
      await invoke("set_ai_features_enabled", { enabled: false });

      setHasKey(false);
      setAiEnabled(false);
      setApiKey("");
      setShowValue(false);
      setLoadedKeyValue("");
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

  const handleShowValue = async () => {
    // If we haven't loaded the key yet, authenticate first
    if (!loadedKeyValue) {
      setIsAuthenticating(true);
      try {
        const key = await invoke("get_anthropic_api_key_authenticated");
        setLoadedKeyValue(key);
        setShowValue(true); // Auto-show after authentication
      } catch (err) {
        console.error("Authentication failed:", err);
        showMessage(err.toString(), "error");
      } finally {
        setIsAuthenticating(false);
      }
    } else {
      // Just toggle visibility
      setShowValue(!showValue);
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
                <div className="space-y-3">
                  <div className="px-4 py-2.5 bg-gray-50 rounded-lg border border-gray-200 font-mono text-sm overflow-x-auto">
                    {showValue ? (
                      <div className="text-gray-900 break-all">{loadedKeyValue || "(empty)"}</div>
                    ) : (
                      <div className="text-gray-500">••••••••••••••••••••</div>
                    )}
                  </div>
                  <div className="flex items-center gap-3">
                    <button
                      onClick={handleShowValue}
                      disabled={isAuthenticating}
                      className="flex-1 px-4 py-2.5 border border-gray-300 text-gray-700 rounded-lg hover:bg-gray-50 disabled:opacity-50 disabled:cursor-not-allowed transition-colors text-sm font-medium"
                    >
                      {isAuthenticating ? "..." : (showValue ? "Hide" : "Show")}
                    </button>
                    <button
                      onClick={handleRemove}
                      className="flex-1 px-4 py-2.5 border border-red-200 text-red-600 rounded-lg hover:bg-red-50 transition-colors text-sm font-medium"
                    >
                      Remove
                    </button>
                  </div>
                </div>
              ) : (
                <div className="space-y-3">
                  <div className="flex items-center gap-3">
                    <div className="flex-1 relative">
                      <input
                        type="password"
                        value={apiKey}
                        onChange={handleApiKeyChange}
                        placeholder="sk-ant-..."
                        className={`w-full px-4 py-2.5 border rounded-lg focus:outline-none focus:ring-2 focus:border-transparent text-sm ${
                          validationStatus === "valid"
                            ? "border-green-300 focus:ring-green-500"
                            : validationStatus === "invalid"
                            ? "border-red-300 focus:ring-red-500"
                            : validationStatus === "verifying"
                            ? "border-blue-300 focus:ring-blue-500"
                            : "border-gray-300 focus:ring-blue-500"
                        }`}
                        disabled={saving}
                      />
                      {validationStatus && (
                        <div className="absolute right-3 top-1/2 -translate-y-1/2">
                          {validationStatus === "valid" && (
                            <svg className="w-5 h-5 text-green-500" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M5 13l4 4L19 7" />
                            </svg>
                          )}
                          {validationStatus === "invalid" && (
                            <svg className="w-5 h-5 text-red-500" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M6 18L18 6M6 6l12 12" />
                            </svg>
                          )}
                          {validationStatus === "verifying" && (
                            <svg className="w-5 h-5 text-blue-500 animate-spin" fill="none" viewBox="0 0 24 24">
                              <circle className="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" strokeWidth="4"></circle>
                              <path className="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z"></path>
                            </svg>
                          )}
                        </div>
                      )}
                    </div>
                    <button
                      onClick={handleSave}
                      disabled={!apiKey || saving || validationStatus === "invalid"}
                      className="px-6 py-2.5 bg-blue-600 text-white rounded-lg hover:bg-blue-700 disabled:opacity-50 disabled:cursor-not-allowed transition-colors text-sm font-medium"
                    >
                      {saving ? "Verifying..." : "Save"}
                    </button>
                  </div>
                  {validationMessage && (
                    <p className={`text-sm ${
                      validationStatus === "valid"
                        ? "text-green-600"
                        : validationStatus === "invalid"
                        ? "text-red-600"
                        : "text-blue-600"
                    }`}>
                      {validationMessage}
                    </p>
                  )}
                </div>
              )}

              <div className="mt-3 space-y-1.5 text-xs text-gray-500">
                <p>✓ Encrypted and stored locally in ~/.workbooks/app/</p>
                <p>✓ Protected by AES-256-GCM encryption</p>
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
