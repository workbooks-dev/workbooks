import { useState, useEffect, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import ClaudeApprovalModal from "./ClaudeApprovalModal";

export function AiSidebar({ projectRoot, isOpen, onToggle, aiEnabled, onOpenSettings, width, onResizeStart }) {
  const [sessions, setSessions] = useState([]);
  const [activeSession, setActiveSession] = useState(null);
  const [messages, setMessages] = useState([]);
  const [inputMessage, setInputMessage] = useState("");
  const [sending, setSending] = useState(false);
  const [loading, setLoading] = useState(true);
  const [claudeInstalled, setClaudeInstalled] = useState(null);
  const [sessionId, setSessionId] = useState(null);
  const [pendingChanges, setPendingChanges] = useState(null);
  const [pendingResponse, setPendingResponse] = useState(null);
  const [pendingPrompt, setPendingPrompt] = useState(null);
  const messagesEndRef = useRef(null);
  const inputRef = useRef(null);

  useEffect(() => {
    if (aiEnabled) {
      checkClaudeInstallation();
      loadSessions();
    }
  }, [aiEnabled]);

  useEffect(() => {
    if (projectRoot && aiEnabled) {
      loadSessionId();
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

  const loadSessionId = async () => {
    try {
      const sid = await invoke("claude_cli_get_session_id", { projectRoot });
      setSessionId(sid);
    } catch (err) {
      console.error("Failed to load session ID:", err);
    }
  };

  const loadSessions = async () => {
    try {
      setLoading(true);
      const sessionList = await invoke("list_chat_sessions");
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
    } catch (err) {
      console.error("Failed to load session:", err);
    }
  };

  const createNewSession = async () => {
    try {
      const session = await invoke("create_chat_session", {
        title: "New Chat",
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

    const userMessage = {
      role: "user",
      content: inputMessage.trim(),
      timestamp: Date.now(),
    };

    // Add user message immediately
    setMessages((prev) => [...prev, userMessage]);
    const prompt = inputMessage.trim();
    setInputMessage("");
    setSending(true);

    try {
      // Ensure we have a session
      let chatSessionId = activeSession?.id;
      if (!chatSessionId) {
        const newSession = await invoke("create_chat_session", {
          title: prompt.slice(0, 50),
        });
        chatSessionId = newSession.id;
        setActiveSession(newSession);
        setSessions([newSession, ...sessions]);
      }

      // Save user message
      await invoke("add_message_to_session", {
        sessionId: chatSessionId,
        role: "user",
        content: userMessage.content,
      });

      // First, run in plan mode to analyze what Claude wants to do
      const [response, changes] = await invoke("claude_cli_plan", {
        prompt,
        projectRoot,
        sessionId,
      });

      if (changes && changes.length > 0) {
        // Changes detected - show approval modal
        setPendingChanges(changes);
        setPendingResponse(response.result);
        setPendingPrompt(prompt);
      } else {
        // No changes - just show the response
        const assistantMessage = {
          role: "assistant",
          content: response.result,
          timestamp: Date.now(),
        };
        setMessages((prev) => [...prev, assistantMessage]);

        await invoke("add_message_to_session", {
          sessionId: chatSessionId,
          role: "assistant",
          content: response.result,
        });
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
    if (!pendingPrompt || !sessionId) {
      setPendingChanges(null);
      setPendingResponse(null);
      setPendingPrompt(null);
      return;
    }

    setSending(true);

    try {
      // Add placeholder assistant message for streaming
      const assistantMessageIndex = messages.length;
      const assistantMessage = {
        role: "assistant",
        content: "",
        timestamp: Date.now(),
        isStreaming: true,
      };
      setMessages((prev) => [...prev, assistantMessage]);

      let streamedContent = "";

      // Set up event listener for streaming
      const unlisten = await listen("claude-cli-chunk", (event) => {
        const chunk = event.payload;
        streamedContent += chunk;
        setMessages((prev) => {
          const newMessages = [...prev];
          newMessages[assistantMessageIndex] = {
            ...newMessages[assistantMessageIndex],
            content: streamedContent,
          };
          return newMessages;
        });
      });

      // Execute with approved tools
      const response = await invoke("claude_cli_stream", {
        prompt: pendingPrompt,
        projectRoot,
        sessionId,
        allowedTools,
      });

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
      const errorMessage = {
        role: "assistant",
        content: `Error: ${err.toString()}`,
        timestamp: Date.now(),
        isError: true,
      };
      setMessages((prev) => [...prev, errorMessage]);
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
  };

  const handleKeyPress = (e) => {
    if (e.key === "Enter" && !e.shiftKey) {
      e.preventDefault();
      e.stopPropagation();
      handleSend();
    }
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
                  onClick={() => loadSession(session.id)}
                >
                  <span className="text-xs text-gray-700 truncate flex-1">
                    {session.title || "Untitled Chat"}
                  </span>
                  <button
                    onClick={(e) => {
                      e.stopPropagation();
                      deleteSession(session.id);
                    }}
                    className="opacity-0 group-hover:opacity-100 p-1 hover:bg-red-50 rounded transition-all"
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
