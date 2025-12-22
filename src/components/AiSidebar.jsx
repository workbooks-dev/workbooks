import { useState, useEffect, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

export function AiSidebar({ projectRoot, isOpen, onToggle, aiEnabled, onOpenSettings, width, onResizeStart }) {
  const [sessions, setSessions] = useState([]);
  const [activeSession, setActiveSession] = useState(null);
  const [messages, setMessages] = useState([]);
  const [inputMessage, setInputMessage] = useState("");
  const [sending, setSending] = useState(false);
  const [loading, setLoading] = useState(true);
  const messagesEndRef = useRef(null);
  const inputRef = useRef(null);

  useEffect(() => {
    if (aiEnabled) {
      loadSessions();
    }
  }, [aiEnabled]);

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
    setInputMessage("");
    setSending(true);

    try {
      // Ensure engine server is running before sending message
      await invoke("ensure_engine_server");

      // Ensure we have a session
      let sessionId = activeSession?.id;
      if (!sessionId) {
        const newSession = await invoke("create_chat_session", {
          title: inputMessage.trim().slice(0, 50),
        });
        sessionId = newSession.id;
        setActiveSession(newSession);
        setSessions([newSession, ...sessions]);
      }

      // Save user message
      await invoke("add_message_to_session", {
        sessionId,
        role: "user",
        content: userMessage.content,
      });

      // Add placeholder assistant message for streaming
      const assistantMessageIndex = messages.length + 1; // Account for user message just added
      const assistantMessage = {
        role: "assistant",
        content: "",
        timestamp: Date.now(),
        isStreaming: true,
      };
      setMessages((prev) => [...prev, assistantMessage]);

      // Set up event listener for streaming
      const eventName = `agent-stream-${sessionId}`;
      let streamedContent = "";

      const unlisten = await listen(eventName, (event) => {
        const payload = event.payload;

        if (payload.content) {
          // Chunk received - append to the message
          streamedContent += payload.content;
          setMessages((prev) => {
            const newMessages = [...prev];
            newMessages[assistantMessageIndex] = {
              ...newMessages[assistantMessageIndex],
              content: streamedContent,
            };
            return newMessages;
          });
        } else if (payload.complete) {
          // Streaming complete
          console.log("Agent stream complete");
        } else if (payload.error) {
          // Error received
          streamedContent = `Error: ${payload.error}`;
          setMessages((prev) => {
            const newMessages = [...prev];
            newMessages[assistantMessageIndex] = {
              role: "assistant",
              content: streamedContent,
              timestamp: Date.now(),
              isError: true,
            };
            return newMessages;
          });
        }
      });

      // Send to agent (this will stream via events)
      const response = await invoke("send_agent_message", {
        sessionId,
        message: userMessage.content,
        projectRoot,
      });

      // Clean up event listener
      unlisten();

      // Mark streaming as complete
      setMessages((prev) => {
        const newMessages = [...prev];
        if (newMessages[assistantMessageIndex]) {
          delete newMessages[assistantMessageIndex].isStreaming;
        }
        return newMessages;
      });

      // Save assistant message with final content
      await invoke("add_message_to_session", {
        sessionId,
        role: "assistant",
        content: streamedContent || response, // Use streamed content or fallback to response
      });
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

  const handleCancel = async () => {
    if (!activeSession?.id) return;

    try {
      await invoke("cancel_agent_request", {
        sessionId: activeSession.id,
      });
      setSending(false);

      // Add cancellation message
      const cancelMessage = {
        role: "assistant",
        content: "Request cancelled by user",
        timestamp: Date.now(),
        isError: true,
      };
      setMessages((prev) => [...prev, cancelMessage]);
    } catch (err) {
      console.error("Failed to cancel request:", err);
    }
  };

  const handleKeyPress = (e) => {
    if (e.key === "Enter" && !e.shiftKey) {
      e.preventDefault();
      e.stopPropagation(); // Prevent event from bubbling to parent components
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
          <h3 className="text-sm font-semibold text-gray-900">AI Assistant</h3>
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

      {/* AI Disabled State */}
      {!aiEnabled ? (
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
                      d="M19 7l-.867 12.142A2 2 0 0116.138 21H7.862a2 2 0 01-1.995-1.858L5 7m5 4v6m4-6v6m1-10V4a1 1 0 00-1-1h-4a1 1 0 00-1 1v3M4 7h16"
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
            <p>Start a conversation with the AI assistant</p>
            <p className="text-xs mt-1">Ask about your project, get help with code, or automate tasks</p>
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
            <button
              onClick={handleCancel}
              className="px-3 py-1 text-xs bg-red-50 text-red-600 border border-red-200 rounded hover:bg-red-100 transition-colors"
              title="Cancel request"
            >
              Cancel
            </button>
          </div>
        )}

        <div ref={messagesEndRef} />
      </div>

      {/* Input */}
      <div className="border-t border-gray-200 bg-white px-4 py-3">
        <div className="flex gap-2">
          <textarea
            ref={inputRef}
            value={inputMessage}
            onChange={(e) => setInputMessage(e.target.value)}
            onKeyPress={handleKeyPress}
            onKeyDown={(e) => e.stopPropagation()} // Prevent all keyboard events from bubbling
            placeholder="Ask the AI assistant..."
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
  );
}
