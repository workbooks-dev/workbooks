import { useState, useEffect, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import ClaudeApprovalModal from "./ClaudeApprovalModal";

// Helper function to build an enhanced prompt with project context
async function buildEnhancedPrompt(userPrompt, projectRoot) {
  try {
    // Get project context (existing notebooks, project name, etc.)
    const projectContext = await invoke("get_project_context", { projectRoot });
    const notebooksList = projectContext.notebooks.map(nb => nb.relative_path).join(', ');

    const context = `You are helping with a Workbooks project. Workbooks are Jupyter notebooks (.ipynb files) that can be automated and scheduled.

PROJECT CONTEXT:
- Project name: ${projectContext.project_name}
- Project location: ${projectRoot}
- Existing workbooks: ${notebooksList || 'None yet'}

IMPORTANT INSTRUCTIONS:
- When the user asks to create a workbook/notebook, ALWAYS use the Write tool to create a .ipynb file in the project root
- Workbook files should be named descriptively (e.g., "data_analysis.ipynb", "report_generator.ipynb")
- Each workbook MUST have a valid Jupyter notebook structure:
  {
    "cells": [
      {
        "cell_type": "markdown",
        "metadata": {},
        "source": ["# Title\\n", "Description here"]
      },
      {
        "cell_type": "code",
        "execution_count": null,
        "metadata": {},
        "outputs": [],
        "source": ["# Your Python code here"]
      }
    ],
    "metadata": {
      "kernelspec": {
        "display_name": "Python 3",
        "language": "python",
        "name": "python3"
      },
      "language_info": {
        "name": "python",
        "version": "3.11.0"
      }
    },
    "nbformat": 4,
    "nbformat_minor": 4
  }
- Use Python code cells by default
- Include helpful markdown cells to explain what the workbook does
- BE PROACTIVE: If the user asks to create something, create the workbook file immediately

USER REQUEST:
${userPrompt}`;

    return context;
  } catch (err) {
    console.error("Failed to build enhanced prompt:", err);
    // Fallback to basic context if project context fails
    return `You are helping with a Workbooks project at ${projectRoot}. Workbooks are Jupyter notebooks (.ipynb files).

IMPORTANT: When the user asks to create a workbook, use the Write tool to create a .ipynb file with valid Jupyter notebook JSON structure.

USER REQUEST:
${userPrompt}`;
  }
}

// Model configurations with display names and descriptions
const MODEL_OPTIONS = [
  { value: "claude-sonnet-4-5-20250929", label: "Sonnet 4.5", description: "Latest Sonnet (default)" },
  { value: "claude-opus-4-5-20251101", label: "Opus 4.5", description: "Most capable model" },
  { value: "opusplan", label: "Opus Plan", description: "Opus plans, Sonnet executes" },
  { value: "sonnet", label: "Sonnet", description: "Fast & capable" },
  { value: "opus", label: "Opus", description: "Most capable (older)" },
  { value: "haiku", label: "Haiku", description: "Fastest, most economical" },
  { value: "sonnet[1m]", label: "Sonnet 1M", description: "1M context window" },
  { value: "default", label: "Default", description: "Account-optimized" },
];

// Helper to get display name for a model
function getModelDisplayName(modelValue) {
  const option = MODEL_OPTIONS.find(opt => opt.value === modelValue);
  return option ? option.label : modelValue;
}

// Generate a smart title from a user message
function generateSmartTitle(message) {
  // Remove common prefixes
  let cleaned = message
    .replace(/^(can you |could you |please |i want to |i need to |help me |)/i, '')
    .trim();

  // Capitalize first letter
  cleaned = cleaned.charAt(0).toUpperCase() + cleaned.slice(1);

  // Limit to 50 chars, break at word boundary
  if (cleaned.length > 50) {
    cleaned = cleaned.slice(0, 50);
    const lastSpace = cleaned.lastIndexOf(' ');
    if (lastSpace > 20) {
      cleaned = cleaned.slice(0, lastSpace);
    }
    cleaned += '...';
  }

  return cleaned;
}

// Format relative time (e.g., "2m ago", "1h ago", "3d ago")
function formatRelativeTime(timestamp) {
  const now = Date.now() / 1000; // Convert to seconds
  const diff = now - timestamp;

  if (diff < 60) return 'just now';
  if (diff < 3600) return `${Math.floor(diff / 60)}m ago`;
  if (diff < 86400) return `${Math.floor(diff / 3600)}h ago`;
  if (diff < 604800) return `${Math.floor(diff / 86400)}d ago`;

  // Format as date for older items
  const date = new Date(timestamp * 1000);
  return date.toLocaleDateString();
}

export function AiSidebar({ projectRoot, isOpen, onToggle, aiEnabled, onOpenSettings, width, onResizeStart }) {
  const [sessions, setSessions] = useState([]);
  const [activeSession, setActiveSession] = useState(null);
  const [messages, setMessages] = useState([]);
  const [inputMessage, setInputMessage] = useState("");
  const [sending, setSending] = useState(false);
  const [loading, setLoading] = useState(true);
  const [claudeInstalled, setClaudeInstalled] = useState(null);
  const [, setSessionName] = useState(null);
  const [claudeSessionId, setClaudeSessionId] = useState(null); // UUID session ID from Claude CLI
  const [pendingChanges, setPendingChanges] = useState(null);
  const [pendingResponse, setPendingResponse] = useState(null);
  const [pendingPrompt, setPendingPrompt] = useState(null);
  const [pendingEnhancedPrompt, setPendingEnhancedPrompt] = useState(null);
  const [selectedModel, setSelectedModel] = useState("claude-sonnet-4-5-20250929"); // Default to Sonnet 4.5
  const [showModelSelector, setShowModelSelector] = useState(false);
  const [editingSessionId, setEditingSessionId] = useState(null);
  const [editingTitle, setEditingTitle] = useState("");
  const messagesEndRef = useRef(null);
  const inputRef = useRef(null);

  useEffect(() => {
    if (aiEnabled && projectRoot) {
      checkClaudeInstallation();
      loadSessions();
      // Load saved model preference from localStorage as fallback
      const savedModel = localStorage.getItem("claude-model");
      if (savedModel) {
        setSelectedModel(savedModel);
      }
    }
  }, [aiEnabled, projectRoot]);

  // Save model preference to localStorage when it changes
  useEffect(() => {
    if (selectedModel) {
      localStorage.setItem("claude-model", selectedModel);
    }
  }, [selectedModel]);

  useEffect(() => {
    if (projectRoot && aiEnabled) {
      loadSessionName();
    }
  }, [projectRoot, aiEnabled]);

  useEffect(() => {
    // Auto-scroll to bottom when new messages arrive
    messagesEndRef.current?.scrollIntoView({ behavior: "smooth" });
  }, [messages]);

  useEffect(() => {
    // Focus input when sidebar opens
    if (isOpen && inputRef.current) {
      inputRef.current.focus();
    }
  }, [isOpen]);

  const checkClaudeInstallation = async () => {
    try {
      const info = await invoke("check_claude_cli_installed");
      setClaudeInstalled(info);
    } catch (err) {
      console.error("Failed to check Claude installation:", err);
      setClaudeInstalled({ installed: false, version: null, path: null });
    }
  };

  const loadSessionName = async () => {
    try {
      const name = await invoke("claude_cli_get_session_name", { projectRoot });
      setSessionName(name);
    } catch (err) {
      console.error("Failed to load session name:", err);
    }
  };

  const loadSessions = async () => {
    try {
      setLoading(true);
      // Filter sessions by current project
      const sessionList = await invoke("list_chat_sessions", { projectRoot });
      setSessions(sessionList);

      // Load most recent session by default
      if (sessionList.length > 0 && !activeSession) {
        await loadSession(sessionList[0].id);
      }
    } catch (err) {
      console.error("Failed to load chat sessions:", err);
    } finally {
      setLoading(false);
    }
  };

  const loadSession = async (sessionId) => {
    try {
      const session = await invoke("get_chat_session", { sessionId });
      setActiveSession(session);
      setMessages(session.messages || []);

      // Restore model from session if it has one
      if (session.model) {
        setSelectedModel(session.model);
      }
    } catch (err) {
      console.error("Failed to load session:", err);
    }
  };

  const createNewSession = async () => {
    try {
      const session = await invoke("create_chat_session", {
        title: "New Chat",
        model: selectedModel, // Save current model with new session
        projectRoot, // Associate with current project
      });
      setSessions([session, ...sessions]);
      setActiveSession(session);
      setMessages([]);
    } catch (err) {
      console.error("Failed to create session:", err);
    }
  };

  const handleSend = async () => {
    if (!inputMessage.trim() || sending) return;

    // Set sending state immediately to prevent duplicate sends
    setSending(true);

    const userMessage = {
      role: "user",
      content: inputMessage.trim(),
      timestamp: Date.now(),
    };

    // Add user message immediately
    setMessages((prev) => [...prev, userMessage]);
    const prompt = inputMessage.trim();
    setInputMessage("");

    try {
      // Ensure we have a session
      let chatSessionId = activeSession?.id;
      let isNewSession = false;
      if (!chatSessionId) {
        const smartTitle = generateSmartTitle(prompt);
        const newSession = await invoke("create_chat_session", {
          title: smartTitle,
          model: selectedModel, // Save current model with session
          projectRoot, // Associate with current project
        });
        chatSessionId = newSession.id;
        setActiveSession(newSession);
        setSessions([newSession, ...sessions]);
        isNewSession = true;
      }

      // Save user message
      await invoke("add_message_to_session", {
        sessionId: chatSessionId,
        role: "user",
        content: userMessage.content,
      });

      // Auto-update title if this is the first message in an existing session
      if (!isNewSession && messages.length === 0 && activeSession) {
        const smartTitle = generateSmartTitle(prompt);
        await invoke("update_chat_session_title", {
          sessionId: chatSessionId,
          newTitle: smartTitle,
        });
        // Update local state
        setActiveSession({ ...activeSession, title: smartTitle });
        setSessions((prev) =>
          prev.map((s) => (s.id === chatSessionId ? { ...s, title: smartTitle } : s))
        );
      }

      // Build enhanced prompt with project context
      const enhancedPrompt = await buildEnhancedPrompt(prompt, projectRoot);

      // First, run in plan mode to analyze what Claude wants to do
      const [response, changes] = await invoke("claude_cli_plan", {
        prompt: enhancedPrompt,
        projectRoot,
        sessionName: claudeSessionId, // Use UUID session ID if available
        model: selectedModel,
      });

      // Store the session ID returned by Claude CLI for future requests
      if (response.session_id) {
        setClaudeSessionId(response.session_id);
      }

      if (changes && changes.length > 0) {
        // Changes detected - show approval modal
        setPendingChanges(changes);
        setPendingResponse(response.result);
        setPendingPrompt(prompt);
        setPendingEnhancedPrompt(enhancedPrompt);
      } else {
        // No changes - execute directly with streaming
        // Add placeholder assistant message for streaming
        const assistantMessageIndex = messages.length + 1; // +1 because we already added user message
        const assistantMessage = {
          role: "assistant",
          content: "",
          timestamp: Date.now(),
          isStreaming: true,
          progress: [],
        };
        setMessages((prev) => [...prev, assistantMessage]);

        let streamedContent = "";
        let progressEvents = [];

        // Set up event listener for all Claude events
        const unlisten = await listen("claude-cli-event", (event) => {
          const payload = event.payload;
          const eventType = payload.type;

          console.log("[No-changes path] Received event:", JSON.stringify(payload, null, 2));

          // Handle different event types
          if (eventType === "stream_event" && payload.event) {
            const innerEvent = payload.event;

            // Handle content deltas (streaming text)
            if (innerEvent.type === "content_block_delta" && innerEvent.delta?.type === "text_delta") {
              const content = innerEvent.delta.text || "";
              console.log("[No-changes path] Got text chunk:", content);
              streamedContent += content;
              setMessages((prev) => {
                const newMessages = [...prev];
                if (newMessages[assistantMessageIndex]) {
                  newMessages[assistantMessageIndex] = {
                    ...newMessages[assistantMessageIndex],
                    content: streamedContent,
                  };
                }
                return newMessages;
              });
            }
          } else if (eventType === "tool_use" || eventType === "tool_result") {
            const toolName = payload.tool || payload.name || "Unknown";
            const toolInput = payload.input || {};
            const progressMsg = `🔧 Using ${toolName}${toolInput.path ? `: ${toolInput.path}` : ""}`;

            progressEvents.push(progressMsg);
            setMessages((prev) => {
              const newMessages = [...prev];
              if (newMessages[assistantMessageIndex]) {
                newMessages[assistantMessageIndex] = {
                  ...newMessages[assistantMessageIndex],
                  progress: [...progressEvents],
                };
              }
              return newMessages;
            });
          } else if (eventType === "thinking") {
            progressEvents.push("💭 Thinking...");
            setMessages((prev) => {
              const newMessages = [...prev];
              if (newMessages[assistantMessageIndex]) {
                newMessages[assistantMessageIndex] = {
                  ...newMessages[assistantMessageIndex],
                  progress: [...progressEvents],
                };
              }
              return newMessages;
            });
          }
        });

        try {
          // Execute with enhanced prompt
          const streamResponse = await invoke("claude_cli_stream", {
            prompt: enhancedPrompt,
            projectRoot,
            sessionName: claudeSessionId,
            allowedTools: null, // No tool restrictions for simple queries
            model: selectedModel,
          });

          // Clean up listener
          unlisten();

          // Store the session ID
          if (streamResponse.session_id) {
            setClaudeSessionId(streamResponse.session_id);
          }

          // Mark streaming as complete
          setMessages((prev) => {
            const newMessages = [...prev];
            if (newMessages[assistantMessageIndex]) {
              delete newMessages[assistantMessageIndex].isStreaming;
              newMessages[assistantMessageIndex].content = streamedContent || streamResponse.result;
            }
            return newMessages;
          });

          // Save assistant message
          await invoke("add_message_to_session", {
            sessionId: chatSessionId,
            role: "assistant",
            content: streamedContent || streamResponse.result,
          });
        } catch (streamError) {
          console.error("Streaming error:", streamError);
          unlisten();

          // Update message with error
          setMessages((prev) => {
            const newMessages = [...prev];
            if (newMessages[assistantMessageIndex]) {
              newMessages[assistantMessageIndex] = {
                role: "assistant",
                content: `I encountered an error: ${streamError.toString()}\n\nPlease try again or rephrase your request.`,
                timestamp: Date.now(),
                isError: true,
              };
            }
            return newMessages;
          });
        }
      }
    } catch (err) {
      console.error("Failed to send message:", err);
      // Add error message
      const errorMessage = {
        role: "assistant",
        content: `Error: ${err.toString()}`,
        timestamp: Date.now(),
        isError: true,
      };
      setMessages((prev) => [...prev, errorMessage]);
    } finally {
      setSending(false);
    }
  };

  const handleApprove = async (allowedTools) => {
    if (!pendingPrompt) {
      setPendingChanges(null);
      setPendingResponse(null);
      setPendingPrompt(null);
      setPendingEnhancedPrompt(null);
      return;
    }

    setSending(true);

    // Track assistant message index for streaming updates
    const assistantMessageIndex = messages.length;

    try {
      // Add placeholder assistant message for streaming
      const assistantMessage = {
        role: "assistant",
        content: "",
        timestamp: Date.now(),
        isStreaming: true,
        progress: [],  // Track progress events
      };
      setMessages((prev) => [...prev, assistantMessage]);

      let streamedContent = "";
      let progressEvents = [];

      // Set up event listener for all Claude events
      const unlisten = await listen("claude-cli-event", (event) => {
        const payload = event.payload;
        const eventType = payload.type;

        // Handle different event types
        if (eventType === "stream_event" && payload.event) {
          const innerEvent = payload.event;

          // Handle content deltas (streaming text)
          if (innerEvent.type === "content_block_delta" && innerEvent.delta?.type === "text_delta") {
            const content = innerEvent.delta.text || "";
            streamedContent += content;
            setMessages((prev) => {
              const newMessages = [...prev];
              newMessages[assistantMessageIndex] = {
                ...newMessages[assistantMessageIndex],
                content: streamedContent,
              };
              return newMessages;
            });
          }
        } else if (eventType === "tool_use" || eventType === "tool_result") {
          // Tool usage events (Read, Edit, Bash, etc.)
          const toolName = payload.tool || payload.name || "Unknown";
          const toolInput = payload.input || {};
          const progressMsg = `🔧 Using ${toolName}${toolInput.path ? `: ${toolInput.path}` : ""}`;

          progressEvents.push(progressMsg);
          setMessages((prev) => {
            const newMessages = [...prev];
            newMessages[assistantMessageIndex] = {
              ...newMessages[assistantMessageIndex],
              progress: [...progressEvents],
            };
            return newMessages;
          });
        } else if (eventType === "thinking") {
          // Thinking/planning events
          progressEvents.push("💭 Thinking...");
          setMessages((prev) => {
            const newMessages = [...prev];
            newMessages[assistantMessageIndex] = {
              ...newMessages[assistantMessageIndex],
              progress: [...progressEvents],
            };
            return newMessages;
          });
        }
      });

      // Execute with approved tools - use the enhanced prompt with project context
      const response = await invoke("claude_cli_stream", {
        prompt: pendingEnhancedPrompt || pendingPrompt,
        projectRoot,
        sessionName: claudeSessionId, // Use UUID session ID if available
        allowedTools,
        model: selectedModel,
      });

      // Store the session ID returned by Claude CLI for future requests
      if (response.session_id) {
        setClaudeSessionId(response.session_id);
      }

      // Clean up listener
      unlisten();

      // Mark streaming as complete
      setMessages((prev) => {
        const newMessages = [...prev];
        if (newMessages[assistantMessageIndex]) {
          delete newMessages[assistantMessageIndex].isStreaming;
          newMessages[assistantMessageIndex].content = streamedContent || response.result;
        }
        return newMessages;
      });

      // Save assistant message
      if (activeSession?.id) {
        await invoke("add_message_to_session", {
          sessionId: activeSession.id,
          role: "assistant",
          content: streamedContent || response.result,
        });
      }
    } catch (err) {
      console.error("Failed to execute with approval:", err);

      // Clean up the streaming placeholder message and replace with error
      setMessages((prev) => {
        const newMessages = [...prev];
        // Find and update the streaming message (should be the last assistant message)
        const streamingIndex = newMessages.findIndex(
          (msg, idx) => idx >= assistantMessageIndex && msg.role === "assistant" && msg.isStreaming
        );

        if (streamingIndex !== -1) {
          // Update the streaming placeholder with the error
          newMessages[streamingIndex] = {
            role: "assistant",
            content: `I encountered an error: ${err.toString()}\n\nPlease try again or rephrase your request.`,
            timestamp: Date.now(),
            isError: true,
          };
        } else {
          // Fallback: Add error message if streaming placeholder not found
          newMessages.push({
            role: "assistant",
            content: `Error: ${err.toString()}`,
            timestamp: Date.now(),
            isError: true,
          });
        }

        return newMessages;
      });
    } finally {
      setSending(false);
      setPendingChanges(null);
      setPendingResponse(null);
      setPendingPrompt(null);
    }
  };

  const handleDeny = () => {
    // Just show the plan response without executing
    if (pendingResponse) {
      const assistantMessage = {
        role: "assistant",
        content: pendingResponse + "\n\n_Changes denied by user_",
        timestamp: Date.now(),
      };
      setMessages((prev) => [...prev, assistantMessage]);

      if (activeSession?.id) {
        invoke("add_message_to_session", {
          sessionId: activeSession.id,
          role: "assistant",
          content: assistantMessage.content,
        });
      }
    }

    setPendingChanges(null);
    setPendingResponse(null);
    setPendingPrompt(null);
    setPendingEnhancedPrompt(null);
  };

  const handleKeyPress = (e) => {
    if (e.key === "Enter" && !e.shiftKey) {
      e.preventDefault();
      e.stopPropagation();
      handleSend();
    }
  };

  const startRenaming = (session) => {
    setEditingSessionId(session.id);
    setEditingTitle(session.title);
  };

  const saveRename = async () => {
    if (!editingTitle.trim() || !editingSessionId) {
      setEditingSessionId(null);
      return;
    }

    try {
      await invoke("update_chat_session_title", {
        sessionId: editingSessionId,
        newTitle: editingTitle.trim(),
      });

      // Update local state
      setSessions((prev) =>
        prev.map((s) =>
          s.id === editingSessionId ? { ...s, title: editingTitle.trim() } : s
        )
      );

      if (activeSession?.id === editingSessionId) {
        setActiveSession({ ...activeSession, title: editingTitle.trim() });
      }

      setEditingSessionId(null);
      setEditingTitle("");
    } catch (err) {
      console.error("Failed to rename session:", err);
    }
  };

  const cancelRename = () => {
    setEditingSessionId(null);
    setEditingTitle("");
  };

  const deleteSession = async (sessionId) => {
    if (!confirm("Delete this chat session?")) return;

    try {
      await invoke("delete_chat_session", { sessionId });
      setSessions(sessions.filter((s) => s.id !== sessionId));

      if (activeSession?.id === sessionId) {
        setActiveSession(null);
        setMessages([]);
      }
    } catch (err) {
      console.error("Failed to delete session:", err);
    }
  };

  return (
    <>
      {pendingChanges && (
        <ClaudeApprovalModal
          pendingChanges={pendingChanges}
          response={pendingResponse}
          onApprove={handleApprove}
          onDeny={handleDeny}
        />
      )}

      <div
        className={`h-full bg-gray-50 flex overflow-hidden relative ${
          isOpen ? 'border-l border-gray-200' : ''
        }`}
      >
        {/* Resize Handle */}
        {isOpen && (
          <div
            className="absolute left-0 top-0 bottom-0 w-1 cursor-col-resize hover:bg-blue-500 z-10 group"
            onMouseDown={(e) => {
              e.preventDefault();
              onResizeStart();
            }}
            title="Drag to resize"
          >
            <div className="absolute inset-y-0 -left-1 -right-1" />
          </div>
        )}
        <div className="h-full flex flex-col" style={{ width: `${width}px`, opacity: isOpen ? 1 : 0 }}>
        {/* Header */}
        <div className="px-4 py-3 border-b border-gray-200 bg-white flex items-center justify-between">
          <div className="flex items-center gap-2">
            <svg
              className="w-5 h-5 text-blue-600"
              fill="none"
              stroke="currentColor"
              viewBox="0 0 24 24"
            >
              <path
                strokeLinecap="round"
                strokeLinejoin="round"
                strokeWidth={2}
                d="M8 10h.01M12 10h.01M16 10h.01M9 16H5a2 2 0 01-2-2V6a2 2 0 012-2h14a2 2 0 012 2v8a2 2 0 01-2 2h-5l-5 5v-5z"
              />
            </svg>
            <h3 className="text-sm font-semibold text-gray-900">Claude Code</h3>

            {/* Model Selector */}
            <div className="relative ml-2">
              <button
                onClick={() => setShowModelSelector(!showModelSelector)}
                className="px-2 py-1 text-xs font-medium text-gray-700 bg-gray-100 hover:bg-gray-200 rounded transition-colors flex items-center gap-1"
                title="Change model"
              >
                <span>{getModelDisplayName(selectedModel)}</span>
                <svg className="w-3 h-3" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                  <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M19 9l-7 7-7-7" />
                </svg>
              </button>

              {showModelSelector && (
                <div className="absolute top-full left-0 mt-1 bg-white border border-gray-200 rounded-lg shadow-lg z-50 min-w-[220px]">
                  <div className="py-1 max-h-96 overflow-y-auto">
                    {MODEL_OPTIONS.map((model) => (
                      <button
                        key={model.value}
                        onClick={() => {
                          setSelectedModel(model.value);
                          setShowModelSelector(false);
                        }}
                        className={`w-full px-3 py-2 text-left text-sm hover:bg-gray-100 transition-colors ${
                          selectedModel === model.value ? "bg-blue-50 text-blue-700" : "text-gray-700"
                        }`}
                      >
                        <div className="flex items-center justify-between">
                          <div className="flex-1">
                            <div className="font-medium">{model.label}</div>
                            <div className="text-xs text-gray-500 mt-0.5">{model.description}</div>
                          </div>
                          {selectedModel === model.value && (
                            <span className="text-blue-600 ml-2">✓</span>
                          )}
                        </div>
                      </button>
                    ))}
                  </div>
                </div>
              )}
            </div>
          </div>
          <button
            onClick={onToggle}
            className="p-1 hover:bg-gray-100 rounded transition-colors"
            title="Close sidebar"
          >
            <svg
              className="w-4 h-4 text-gray-600"
              fill="none"
              stroke="currentColor"
              viewBox="0 0 24 24"
            >
              <path
                strokeLinecap="round"
                strokeLinejoin="round"
                strokeWidth={2}
                d="M6 18L18 6M6 6l12 12"
              />
            </svg>
          </button>
        </div>

        {/* Claude Not Installed State */}
        {claudeInstalled && !claudeInstalled.installed ? (
          <div className="flex-1 flex items-center justify-center px-6 py-12">
            <div className="text-center max-w-sm">
              <svg
                className="w-16 h-16 mx-auto mb-4 text-amber-300"
                fill="none"
                stroke="currentColor"
                viewBox="0 0 24 24"
              >
                <path
                  strokeLinecap="round"
                  strokeLinejoin="round"
                  strokeWidth={1.5}
                  d="M12 9v2m0 4h.01m-6.938 4h13.856c1.54 0 2.502-1.667 1.732-3L13.732 4c-.77-1.333-2.694-1.333-3.464 0L3.34 16c-.77 1.333.192 3 1.732 3z"
                />
              </svg>
              <h3 className="text-lg font-semibold text-gray-900 mb-2">
                Claude Code CLI Not Installed
              </h3>
              <p className="text-sm text-gray-600 mb-4">
                To use the AI assistant, please install Claude Code CLI first
              </p>
              <a
                href="https://claude.com/claude-code"
                target="_blank"
                rel="noopener noreferrer"
                className="inline-block px-4 py-2 bg-blue-600 text-white rounded-lg hover:bg-blue-700 transition-colors text-sm font-medium"
              >
                Install Claude Code
              </a>
            </div>
          </div>
        ) : !aiEnabled ? (
          <div className="flex-1 flex items-center justify-center px-6 py-12">
            <div className="text-center max-w-sm">
              <svg
                className="w-16 h-16 mx-auto mb-4 text-gray-300"
                fill="none"
                stroke="currentColor"
                viewBox="0 0 24 24"
              >
                <path
                  strokeLinecap="round"
                  strokeLinejoin="round"
                  strokeWidth={1.5}
                  d="M12 15v2m-6 4h12a2 2 0 002-2v-6a2 2 0 00-2-2H6a2 2 0 00-2 2v6a2 2 0 002 2zm10-10V7a4 4 0 00-8 0v4h8z"
                />
              </svg>
              <h3 className="text-lg font-semibold text-gray-900 mb-2">
                AI Features Disabled
              </h3>
              <p className="text-sm text-gray-600 mb-4">
                Enable AI features in Settings to use the AI assistant
              </p>
              <button
                onClick={onOpenSettings}
                className="px-4 py-2 bg-blue-600 text-white rounded-lg hover:bg-blue-700 transition-colors text-sm font-medium"
              >
                Open Settings
              </button>
            </div>
          </div>
        ) : (
          <>
            {/* Session List */}
            <div className="border-b border-gray-200 bg-white">
          <div className="px-4 py-2">
            <button
              onClick={createNewSession}
              className="w-full px-3 py-2 text-xs font-medium text-blue-600 bg-blue-50 hover:bg-blue-100 rounded transition-colors flex items-center justify-center gap-2"
            >
              <span>+</span>
              <span>New Chat</span>
            </button>
          </div>

          {loading ? (
            <div className="px-4 py-3 text-xs text-gray-500">Loading...</div>
          ) : sessions.length > 0 ? (
            <div className="max-h-32 overflow-y-auto">
              {sessions.slice(0, 5).map((session) => (
                <div
                  key={session.id}
                  className={`px-4 py-2 cursor-pointer hover:bg-gray-50 transition-colors flex items-center justify-between group ${
                    activeSession?.id === session.id ? "bg-blue-50" : ""
                  }`}
                  onClick={() => {
                    if (editingSessionId !== session.id) {
                      loadSession(session.id);
                    }
                  }}
                >
                  <div className="flex-1 min-w-0">
                    {editingSessionId === session.id ? (
                      <input
                        type="text"
                        value={editingTitle}
                        onChange={(e) => setEditingTitle(e.target.value)}
                        onKeyPress={(e) => {
                          e.stopPropagation();
                          if (e.key === "Enter") {
                            saveRename();
                          } else if (e.key === "Escape") {
                            cancelRename();
                          }
                        }}
                        onBlur={saveRename}
                        onClick={(e) => e.stopPropagation()}
                        className="w-full px-2 py-1 text-xs text-gray-900 border border-blue-500 rounded focus:outline-none focus:ring-1 focus:ring-blue-500"
                        autoFocus
                      />
                    ) : (
                      <>
                        <div className="flex items-center gap-1">
                          <div className="text-xs font-medium text-gray-900 truncate">
                            {session.title || "Untitled Chat"}
                          </div>
                          <button
                            onClick={(e) => {
                              e.stopPropagation();
                              startRenaming(session);
                            }}
                            className="opacity-0 group-hover:opacity-100 p-0.5 hover:bg-gray-200 rounded transition-all"
                            title="Rename session"
                          >
                            <svg
                              className="w-3 h-3 text-gray-600"
                              fill="none"
                              stroke="currentColor"
                              viewBox="0 0 24 24"
                            >
                              <path
                                strokeLinecap="round"
                                strokeLinejoin="round"
                                strokeWidth={2}
                                d="M15.232 5.232l3.536 3.536m-2.036-5.036a2.5 2.5 0 113.536 3.536L6.5 21.036H3v-3.572L16.732 3.732z"
                              />
                            </svg>
                          </button>
                        </div>
                        <div className="text-xs text-gray-500 mt-0.5">
                          {formatRelativeTime(session.updated_at)}
                          {session.model && ` · ${getModelDisplayName(session.model)}`}
                        </div>
                      </>
                    )}
                  </div>
                  {editingSessionId !== session.id && (
                    <button
                      onClick={(e) => {
                        e.stopPropagation();
                        deleteSession(session.id);
                      }}
                      className="opacity-0 group-hover:opacity-100 p-1 hover:bg-red-50 rounded transition-all ml-2 flex-shrink-0"
                      title="Delete session"
                    >
                      <svg
                        className="w-3 h-3 text-red-600"
                        fill="none"
                        stroke="currentColor"
                        viewBox="0 0 24 24"
                      >
                        <path
                          strokeLinecap="round"
                          strokeLinejoin="round"
                          strokeWidth={2}
                          d="M19 7l-.867 12.142A2 0 0116.138 21H7.862a2 2 0 01-1.995-1.858L5 7m5 4v6m4-6v6m1-10V4a1 1 0 00-1-1h-4a1 1 0 00-1 1v3M4 7h16"
                        />
                      </svg>
                    </button>
                  )}
                </div>
              ))}
            </div>
          ) : null}
        </div>

        {/* Messages */}
        <div className="flex-1 overflow-y-auto px-4 py-4 space-y-4">
          {messages.length === 0 ? (
            <div className="text-center py-12 text-gray-400 text-sm">
              <svg
                className="w-12 h-12 mx-auto mb-3 text-gray-300"
                fill="none"
                stroke="currentColor"
                viewBox="0 0 24 24"
              >
                <path
                  strokeLinecap="round"
                  strokeLinejoin="round"
                  strokeWidth={1.5}
                  d="M8 10h.01M12 10h.01M16 10h.01M9 16H5a2 2 0 01-2-2V6a2 2 0 012-2h14a2 2 0 012 2v8a2 2 0 01-2 2h-5l-5 5v-5z"
                />
              </svg>
              <p>Ask Claude Code to help with your project</p>
              <p className="text-xs mt-1">Code analysis, debugging, file operations, and more</p>
            </div>
          ) : (
            messages.map((msg, idx) => (
              <div
                key={idx}
                className={`flex ${
                  msg.role === "user" ? "justify-end" : "justify-start"
                }`}
              >
                <div
                  className={`max-w-[85%] rounded-lg px-3 py-2 text-sm ${
                    msg.role === "user"
                      ? "bg-blue-600 text-white"
                      : msg.isError
                      ? "bg-red-50 text-red-800 border border-red-200"
                      : "bg-white text-gray-800 border border-gray-200"
                  }`}
                >
                  {/* Show progress events */}
                  {msg.progress && msg.progress.length > 0 && (
                    <div className="mb-2 space-y-1 text-xs text-gray-600 border-b border-gray-200 pb-2">
                      {msg.progress.map((progress, pIdx) => (
                        <div key={pIdx} className="flex items-center gap-1">
                          <span>{progress}</span>
                        </div>
                      ))}
                    </div>
                  )}

                  <pre className="whitespace-pre-wrap font-sans">
                    {msg.content}
                    {msg.isStreaming && (
                      <span className="inline-block w-2 h-4 ml-1 bg-gray-800 animate-pulse"></span>
                    )}
                  </pre>
                </div>
              </div>
            ))
          )}

          {sending && (
            <div className="flex justify-start items-center gap-2">
              <div className="bg-white text-gray-800 border border-gray-200 rounded-lg px-3 py-2 text-sm">
                <div className="flex gap-1">
                  <div className="w-2 h-2 bg-gray-400 rounded-full animate-bounce" style={{ animationDelay: "0ms" }}></div>
                  <div className="w-2 h-2 bg-gray-400 rounded-full animate-bounce" style={{ animationDelay: "150ms" }}></div>
                  <div className="w-2 h-2 bg-gray-400 rounded-full animate-bounce" style={{ animationDelay: "300ms" }}></div>
                </div>
              </div>
            </div>
          )}

          <div ref={messagesEndRef} />
        </div>

        {/* Input */}
        <div className="border-t border-gray-200 bg-white px-4 py-3">
          {claudeInstalled && claudeInstalled.version && (
            <div className="text-xs text-gray-500 mb-2">
              Claude Code v{claudeInstalled.version}
            </div>
          )}
          <div className="flex gap-2">
            <textarea
              ref={inputRef}
              value={inputMessage}
              onChange={(e) => setInputMessage(e.target.value)}
              onKeyPress={handleKeyPress}
              onKeyDown={(e) => e.stopPropagation()}
              placeholder="Ask Claude Code..."
              className="flex-1 px-3 py-2 border border-gray-300 rounded-lg resize-none focus:outline-none focus:ring-2 focus:ring-blue-500 text-sm"
              rows={2}
              disabled={sending}
            />
            <button
              onClick={handleSend}
              disabled={!inputMessage.trim() || sending}
              className="px-4 bg-blue-600 text-white rounded-lg hover:bg-blue-700 disabled:opacity-50 disabled:cursor-not-allowed transition-colors self-end"
            >
              <svg
                className="w-5 h-5"
                fill="none"
                stroke="currentColor"
                viewBox="0 0 24 24"
              >
                <path
                  strokeLinecap="round"
                  strokeLinejoin="round"
                  strokeWidth={2}
                  d="M12 19l9 2-9-18-9 18 9-2zm0 0v-8"
                />
              </svg>
            </button>
          </div>
          <p className="text-xs text-gray-500 mt-2">
            Press Enter to send, Shift+Enter for new line
          </p>
        </div>
          </>
        )}
        </div>
      </div>
    </>
  );
}
