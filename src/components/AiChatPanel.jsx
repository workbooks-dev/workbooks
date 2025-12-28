import { useState, useEffect, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";

export function AiChatPanel({ projectRoot, aiEnabled, onOpenSettings, focusedFile, onOpenFile, onRequestNotebookApproval, initialSession }) {
  const [sessions, setSessions] = useState([]);
  const [activeSession, setActiveSession] = useState(null);
  const [messages, setMessages] = useState([]);
  const [inputMessage, setInputMessage] = useState("");
  const [sending, setSending] = useState(false);
  const [loading, setLoading] = useState(true);
  const [claudeInstalled, setClaudeInstalled] = useState(null);
  const [sessionName, setSessionName] = useState(null);
  const [showHistory, setShowHistory] = useState(false);
  const [projectContext, setProjectContext] = useState(null);
  const [selectedModel, setSelectedModel] = useState("sonnet"); // Default to sonnet
  const [showModelSelector, setShowModelSelector] = useState(false);
  const [chatFilter, setChatFilter] = useState("");
  const [filesUsed, setFilesUsed] = useState(new Set()); // Track files used in conversation
  const messagesEndRef = useRef(null);
  const inputRef = useRef(null);

  useEffect(() => {
    if (aiEnabled) {
      checkClaudeInstallation();
      loadSessions();
      // Load saved model preference
      const savedModel = localStorage.getItem("claude-model");
      if (savedModel) {
        setSelectedModel(savedModel);
      }
    }
  }, [aiEnabled]);

  // Save model preference when it changes
  useEffect(() => {
    if (selectedModel) {
      localStorage.setItem("claude-model", selectedModel);
    }
  }, [selectedModel]);

  useEffect(() => {
    if (projectRoot && aiEnabled) {
      loadSessionName();
      loadProjectContext();
    }
  }, [projectRoot, aiEnabled]);

  // Handle initial session from project
  useEffect(() => {
    if (initialSession && aiEnabled) {
      console.log("Loading initial session from project:", initialSession);
      setActiveSession(initialSession);
      setMessages(initialSession.messages || []);

      // Update sessions list if needed
      const sessionExists = sessions.some(s => s.id === initialSession.id);
      if (!sessionExists) {
        setSessions([initialSession, ...sessions]);
      }

      // Restore model from session if available
      if (initialSession.model) {
        setSelectedModel(initialSession.model);
      }
    }
  }, [initialSession, aiEnabled]);

  const loadProjectContext = async () => {
    try {
      const context = await invoke("get_project_context", { projectRoot });
      setProjectContext(context);
    } catch (err) {
      console.error("Failed to load project context:", err);
    }
  };

  const buildSystemContext = () => {
    if (!projectContext) return "";

    const notebooksList = projectContext.notebooks.length > 0
      ? projectContext.notebooks.map(nb => `  - ${nb.relative_path}`).join("\n")
      : "  (No notebooks found yet)";

    return `# Workbooks Project Context

**Project:** ${projectContext.project_name}
**Purpose:** Building automations with Jupyter notebooks

## Existing Notebooks
${notebooksList}

## Your Role
You are helping build automations using Jupyter notebooks (.ipynb files) in the Workbooks app. When a user asks to automate something:

1. **Check for existing notebooks first:**
   - Look at the list of existing notebooks above
   - If a notebook exists that might handle this automation, read its cells using the Read tool
   - Tell the user what you found and ask if they want to modify it or create a new one

2. **Create new notebooks when needed:**
   - If no relevant notebook exists, offer to create one
   - Use the Write tool to create a new .ipynb file in the project root
   - Start with a basic notebook structure with cells for the automation

3. **Running notebooks to see output:**
   - After creating or modifying a notebook, you can RUN it to test that it works
   - Use the Bash tool to execute: \`workbooks run <notebook-file.ipynb>\`
   - This will execute all cells and show you the output
   - Based on the output, you can iterate and improve the notebook
   - This is how you "sharpen" automations - test them and refine based on real results

4. **Notebook best practices:**
   - Each automation should be in its own notebook
   - Use descriptive names like "quickbooks_sync.ipynb" or "data_processing.ipynb"
   - Include markdown cells explaining what the automation does
   - Use the workbooks state API to share data between notebooks if needed

5. **Be proactive:**
   - Don't ask too many questions - make reasonable assumptions
   - After creating a notebook, offer to run it to verify it works
   - If you see a notebook that looks relevant, read it and tell the user about it
   - Suggest improvements to existing automations when appropriate

Remember: You have access to all files in this project via the Read, Write, Edit, Glob, and Grep tools. Use them to understand and build automations. You can also execute notebooks with \`workbooks run\` to test them!`;
  };

  useEffect(() => {
    // Auto-scroll to bottom when new messages arrive
    messagesEndRef.current?.scrollIntoView({ behavior: "smooth" });
  }, [messages]);

  useEffect(() => {
    // Focus input when component mounts
    if (inputRef.current) {
      inputRef.current.focus();
    }
  }, []);

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
      setShowHistory(false);
    } catch (err) {
      console.error("Failed to load session:", err);
    }
  };

  const createNewSession = async () => {
    try {
      const session = await invoke("create_chat_session", {
        title: "New Chat",
        model: selectedModel,
        projectRoot: projectRoot || null,
      });
      setSessions([session, ...sessions]);
      setActiveSession(session);
      setMessages([]);
      setFilesUsed(new Set());
      setShowHistory(false);
    } catch (err) {
      console.error("Failed to create session:", err);
    }
  };

  /**
   * Generate an AI-based title for the chat session
   * Uses Claude to analyze the conversation and files used
   */
  const generateChatTitle = async (userMsg, assistantMsg, files) => {
    try {
      const filesContext = files.size > 0
        ? `\nFiles involved: ${Array.from(files).map(f => {
            const parts = f.split('/');
            return parts[parts.length - 1]; // Just filename
          }).join(', ')}`
        : '';

      const prompt = `Based on this conversation, generate a SHORT (max 40 chars) descriptive title. Use the format "Topic - Files" if files are involved, otherwise just "Topic".

User: ${userMsg.slice(0, 200)}
Assistant: ${assistantMsg.slice(0, 200)}${filesContext}

Return ONLY the title, nothing else. Examples:
- "Fix login bug - auth.js"
- "Add dark mode"
- "Database migration - schema.sql"`;

      const response = await invoke("claude_cli_chat", {
        prompt,
        projectRoot: null,
        model: "haiku", // Use fast model for title generation
      });

      // Extract just the title from response
      let title = response.result?.trim() || "Chat";

      // Remove quotes if present
      title = title.replace(/^["']|["']$/g, '');

      // Truncate if too long
      if (title.length > 50) {
        title = title.slice(0, 47) + "...";
      }

      return title;
    } catch (err) {
      console.error("Failed to generate chat title:", err);
      // Fallback to first message
      return userMsg.slice(0, 40) + (userMsg.length > 40 ? "..." : "");
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

    // Build comprehensive context
    let promptWithContext = inputMessage.trim();

    // Add project context (system-level instructions)
    const systemContext = buildSystemContext();
    if (systemContext) {
      promptWithContext = `${systemContext}\n\n---\n\nUser Request: ${inputMessage.trim()}`;
    }

    // Add focused file context if available
    if (focusedFile) {
      promptWithContext = `${systemContext}\n\n[Focused file: ${focusedFile.path}]\n\nUser Request: ${inputMessage.trim()}`;
    }

    // Add user message immediately
    setMessages((prev) => [...prev, userMessage]);
    const prompt = promptWithContext;
    setInputMessage("");

    // Track assistant message index for streaming updates
    const assistantMessageIndex = messages.length + 1; // +1 for user message we just added

    try {
      // Ensure we have a session
      let chatSessionId = activeSession?.id;
      let isNewSession = false;
      if (!chatSessionId) {
        // Create session with a descriptive title from the first message
        const title = inputMessage.trim().slice(0, 50) + (inputMessage.trim().length > 50 ? "..." : "");
        const newSession = await invoke("create_chat_session", {
          title,
          model: selectedModel,
          projectRoot: projectRoot || null,
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

      // Update title for "New Chat" sessions after first message
      if (!isNewSession && activeSession?.title === "New Chat") {
        const newTitle = inputMessage.trim().slice(0, 50) + (inputMessage.trim().length > 50 ? "..." : "");
        await invoke("update_chat_session_title", {
          sessionId: chatSessionId,
          newTitle,
        });
        // Update local state
        setActiveSession({ ...activeSession, title: newTitle });
        setSessions(sessions.map(s => s.id === chatSessionId ? { ...s, title: newTitle } : s));
      }

      // Pre-approve common tools for Workbooks AI chat
      // Users expect Claude to be able to read/write files and run commands without permission prompts
      const allowedTools = ["Read", "Write", "Edit", "Bash", "Glob", "Grep", "Task"];

      // Add placeholder assistant message for streaming
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
      let conversationFiles = new Set(filesUsed);

      // Track pending notebook modifications (save version BEFORE Claude modifies)
      const pendingNotebookModifications = new Map(); // filePath -> oldNotebookContent

      // Track active tool executions to know when they complete
      const activeToolExecutions = new Map(); // contentBlockIndex -> { toolName, toolInput }

      // Set up event listener for all Claude events
      const unlisten = await listen("claude-cli-event", (event) => {
        const payload = event.payload;
        const eventType = payload.type;

        // Debug: Log all events to see what Claude CLI is emitting
        console.log("Claude CLI event:", JSON.stringify(payload, null, 2));

        // Handle different event types
        if (eventType === "stream_event") {
          // Claude CLI streaming events
          const innerEvent = payload.event;
          if (innerEvent && innerEvent.type === "content_block_delta") {
            // Extract text delta from streaming content
            const delta = innerEvent.delta;
            if (delta && delta.text) {
              streamedContent += delta.text;
              setMessages((prev) => {
                const newMessages = [...prev];
                newMessages[assistantMessageIndex] = {
                  ...newMessages[assistantMessageIndex],
                  content: streamedContent,
                };
                return newMessages;
              });
            }
          }
        } else if (eventType === "content") {
          // Fallback for simplified content events (backwards compatibility)
          const content = payload.content || "";
          streamedContent += content;
          setMessages((prev) => {
            const newMessages = [...prev];
            newMessages[assistantMessageIndex] = {
              ...newMessages[assistantMessageIndex],
              content: streamedContent,
            };
            return newMessages;
          });
        } else if (eventType === "stream_event" && payload.event && payload.event.type === "content_block_start") {
          // Tool use start event
          const innerEvent = payload.event;
          if (innerEvent.content_block && innerEvent.content_block.type === "tool_use") {
            const toolBlock = innerEvent.content_block;
            const toolName = toolBlock.name || "Unknown";
            const toolInput = toolBlock.input || {};
            const blockIndex = innerEvent.index;

            // Track this tool execution
            activeToolExecutions.set(blockIndex, { toolName, toolInput });

            // Handle notebook modifications BEFORE Claude modifies
            if (toolName === "Write" || toolName === "Edit" || toolName === "NotebookEdit") {
              const filePath = toolInput.file_path || toolInput.notebook_path;
              console.log("🔍 Detected notebook tool:", toolName, "for file:", filePath);

              if (filePath && filePath.endsWith(".ipynb") && onRequestNotebookApproval) {
                console.log("✅ Will track changes for notebook:", filePath);
                // Save current state BEFORE Claude changes it
                (async () => {
                  try {
                    const fileExists = await invoke("read_workbook", { workbookPath: filePath })
                      .then(() => true)
                      .catch(() => false);

                    if (fileExists) {
                      const currentContent = await invoke("read_workbook", { workbookPath: filePath });
                      pendingNotebookModifications.set(filePath, currentContent);
                      console.log("💾 Saved old version of notebook:", filePath);
                    } else {
                      pendingNotebookModifications.set(filePath, JSON.stringify({
                        cells: [],
                        metadata: {},
                        nbformat: 4,
                        nbformat_minor: 5
                      }));
                      console.log("📝 Notebook doesn't exist yet, will track as new file:", filePath);
                    }
                  } catch (error) {
                    console.error("Failed to save notebook state before modification:", error);
                  }
                })();
              } else {
                console.log("⚠️ Not tracking - filePath:", filePath, "ends with .ipynb:", filePath?.endsWith(".ipynb"), "has approval callback:", !!onRequestNotebookApproval);
              }
            }

            // Track files used in the conversation
            if (toolInput.file_path) {
              conversationFiles.add(toolInput.file_path);
              setFilesUsed(new Set(conversationFiles));
            }

            // Show progress indicator
            let progressMsg = "";
            if (toolName === "Read") {
              progressMsg = `📖 Reading ${toolInput.file_path || "file"}`;
            } else if (toolName === "Edit") {
              progressMsg = `✏️ Editing ${toolInput.file_path || "file"}`;
            } else if (toolName === "Write") {
              progressMsg = `📝 Writing ${toolInput.file_path || "file"}`;
            } else if (toolName === "Bash") {
              const cmd = toolInput.command || "";
              const shortCmd = cmd.length > 40 ? cmd.slice(0, 40) + "..." : cmd;
              progressMsg = `⚙️ Running: ${shortCmd}`;
            } else if (toolName === "Glob") {
              progressMsg = `🔍 Searching for ${toolInput.pattern || "files"}`;
            } else if (toolName === "Grep") {
              progressMsg = `🔎 Searching for "${toolInput.pattern || "pattern"}"`;
            } else {
              progressMsg = `🔧 ${toolName}`;
            }

            if (progressMsg) {
              progressEvents.push(progressMsg);
              setMessages((prev) => {
                const newMessages = [...prev];
                newMessages[assistantMessageIndex] = {
                  ...newMessages[assistantMessageIndex],
                  progress: [...progressEvents],
                };
                return newMessages;
              });
            }
          }
        } else if (eventType === "stream_event" && payload.event && payload.event.type === "content_block_stop") {
          // Tool execution complete - trigger diff modal for notebook modifications
          const innerEvent = payload.event;
          const blockIndex = innerEvent.index;

          const toolExecution = activeToolExecutions.get(blockIndex);
          if (toolExecution) {
            const { toolName, toolInput } = toolExecution;
            console.log("🏁 Tool execution completed:", toolName, "at blockIndex:", blockIndex);

            // Check if this was a notebook modification
            if ((toolName === "Write" || toolName === "Edit" || toolName === "NotebookEdit") && (toolInput.file_path || toolInput.notebook_path)) {
              const filePath = toolInput.file_path || toolInput.notebook_path;
              console.log("📓 Notebook tool completed for:", filePath);

              if (filePath.endsWith(".ipynb") && onRequestNotebookApproval) {
                console.log("🚀 Triggering approval workflow for:", filePath);
                // Notebook modification completed - show diff modal
                (async () => {
                  try {
                    // Get the old version we saved before modification
                    const oldContent = pendingNotebookModifications.get(filePath);
                    if (!oldContent) {
                      console.warn("⚠️ No old version found for", filePath, "- skipping diff");
                      // Fall back to just opening the file
                      if (onOpenFile) {
                        onOpenFile(filePath, "workbook");
                      }
                      return;
                    }

                    console.log("📖 Reading new version of:", filePath);
                    // Read the new (modified) version
                    const newContent = await invoke("read_workbook", { workbookPath: filePath });

                    // Parse both versions
                    const oldNotebook = JSON.parse(oldContent);
                    const newNotebook = JSON.parse(newContent);
                    console.log("📊 Old notebook cells:", oldNotebook.cells?.length, "New notebook cells:", newNotebook.cells?.length);

                    // Save the old version to version history for safety
                    if (projectRoot) {
                      await invoke("save_notebook_version", {
                        projectRoot: projectRoot,
                        workbookPath: filePath,
                      });
                      console.log("💾 Saved version to history");
                    }

                    // Trigger the approval flow
                    console.log("🎯 Calling onRequestNotebookApproval callback");
                    onRequestNotebookApproval(filePath, oldNotebook, newNotebook);

                    // Clean up
                    pendingNotebookModifications.delete(filePath);
                    console.log("✅ Approval workflow triggered successfully");
                  } catch (error) {
                    console.error("Failed to handle notebook modification:", error);
                    // On error, fall back to just opening the file
                    if (onOpenFile) {
                      onOpenFile(filePath, "workbook");
                    }
                    pendingNotebookModifications.delete(filePath);
                  }
                })();
              } else if (onOpenFile) {
                // Non-notebook file - auto-open as before
                let fileType = "file";
                if (filePath.endsWith(".py")) {
                  fileType = "python";
                }
                onOpenFile(filePath, fileType);
              }
            }

            // Clean up
            activeToolExecutions.delete(blockIndex);
          }
        } else if (eventType === "tool_use" || eventType === "tool_result") {
          // Fallback for legacy/simplified tool events
          const toolName = payload.tool || payload.name || "Unknown";
          const toolInput = payload.input || {};

          // Track files used in the conversation
          if (toolInput.file_path) {
            conversationFiles.add(toolInput.file_path);
            setFilesUsed(new Set(conversationFiles));
          }

          // BEFORE Claude modifies a notebook, save the current version
          if (eventType === "tool_use" && (toolName === "Write" || toolName === "Edit") && toolInput.file_path) {
            const filePath = toolInput.file_path;

            if (filePath.endsWith(".ipynb") && onRequestNotebookApproval) {
              // This is a notebook modification - save current state BEFORE Claude changes it
              (async () => {
                try {
                  // Check if file exists
                  const fileExists = await invoke("read_workbook", { workbookPath: filePath })
                    .then(() => true)
                    .catch(() => false);

                  if (fileExists) {
                    // File exists - save current content as old version
                    const currentContent = await invoke("read_workbook", { workbookPath: filePath });
                    pendingNotebookModifications.set(filePath, currentContent);
                  } else {
                    // New file - use empty notebook as old version
                    pendingNotebookModifications.set(filePath, JSON.stringify({
                      cells: [],
                      metadata: {},
                      nbformat: 4,
                      nbformat_minor: 5
                    }));
                  }
                } catch (error) {
                  console.error("Failed to save notebook state before modification:", error);
                }
              })();
            }
          }

          // AFTER Claude modifies a notebook, show the diff
          if (eventType === "tool_result" && (toolName === "Write" || toolName === "Edit") && toolInput.file_path) {
            const filePath = toolInput.file_path;

            if (filePath.endsWith(".ipynb") && onRequestNotebookApproval) {
              // Notebook modification - trigger approval flow
              (async () => {
                try {
                  // Get the old version we saved before modification
                  const oldContent = pendingNotebookModifications.get(filePath);
                  if (!oldContent) {
                    console.warn("No old version found for", filePath, "- skipping diff");
                    // Fall back to just opening the file
                    if (onOpenFile) {
                      onOpenFile(filePath, "workbook");
                    }
                    return;
                  }

                  // Read the new (modified) version
                  const newContent = await invoke("read_workbook", { workbookPath: filePath });

                  // Parse both versions
                  const oldNotebook = JSON.parse(oldContent);
                  const newNotebook = JSON.parse(newContent);

                  // Save the old version to version history for safety
                  if (projectRoot) {
                    await invoke("save_notebook_version", {
                      projectRoot: projectRoot,
                      workbookPath: filePath,
                    });
                  }

                  // Trigger the approval flow
                  onRequestNotebookApproval(filePath, oldNotebook, newNotebook);

                  // Clean up
                  pendingNotebookModifications.delete(filePath);
                } catch (error) {
                  console.error("Failed to handle notebook modification:", error);
                  // On error, fall back to just opening the file
                  if (onOpenFile) {
                    onOpenFile(filePath, "workbook");
                  }
                  pendingNotebookModifications.delete(filePath);
                }
              })();
            } else if (onOpenFile) {
              // Non-notebook file - auto-open as before
              let fileType = "file";
              if (filePath.endsWith(".py")) {
                fileType = "python";
              }
              onOpenFile(filePath, fileType);
            }
          }

          // Build detailed progress message based on tool type
          let progressMsg = "";
          if (toolName === "Read") {
            progressMsg = `📖 Reading ${toolInput.file_path || "file"}`;
          } else if (toolName === "Edit") {
            progressMsg = `✏️ Editing ${toolInput.file_path || "file"}`;
          } else if (toolName === "Write") {
            progressMsg = `📝 Writing ${toolInput.file_path || "file"}`;
          } else if (toolName === "Bash") {
            const cmd = toolInput.command || "";
            const shortCmd = cmd.length > 40 ? cmd.slice(0, 40) + "..." : cmd;
            progressMsg = `⚙️ Running: ${shortCmd}`;
          } else if (toolName === "Glob") {
            progressMsg = `🔍 Searching for ${toolInput.pattern || "files"}`;
          } else if (toolName === "Grep") {
            progressMsg = `🔎 Searching for "${toolInput.pattern || "pattern"}"`;
          } else {
            progressMsg = `🔧 ${toolName}${toolInput.path ? `: ${toolInput.path}` : ""}`;
          }

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

      // Execute with pre-approved tools (no permission modal)
      const response = await invoke("claude_cli_stream", {
        prompt,
        projectRoot,
        sessionName,
        allowedTools,
        model: selectedModel,
      });

      // Clean up listener
      unlisten();

      // FALLBACK: Check for any pending notebook modifications that weren't triggered during streaming
      // This handles cases where the event structure didn't match our expectations
      console.log("Checking for pending notebook modifications:", pendingNotebookModifications.size);
      if (pendingNotebookModifications.size > 0) {
        console.log("Found pending modifications, triggering approval flow...");
        for (const [filePath, oldContent] of pendingNotebookModifications.entries()) {
          try {
            // Read the new (modified) version
            const newContent = await invoke("read_workbook", { workbookPath: filePath });

            // Parse both versions
            const oldNotebook = JSON.parse(oldContent);
            const newNotebook = JSON.parse(newContent);

            // Save the old version to version history for safety
            if (projectRoot) {
              await invoke("save_notebook_version", {
                projectRoot: projectRoot,
                workbookPath: filePath,
              });
            }

            // Trigger the approval flow
            onRequestNotebookApproval(filePath, oldNotebook, newNotebook);
          } catch (error) {
            console.error("Failed to handle notebook modification:", error);
            // On error, fall back to just opening the file
            if (onOpenFile) {
              onOpenFile(filePath, "workbook");
            }
          }
        }
        // Clear the pending modifications
        pendingNotebookModifications.clear();
      }

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
      await invoke("add_message_to_session", {
        sessionId: chatSessionId,
        role: "assistant",
        content: streamedContent || response.result,
      });

      // Generate AI-based title for first exchange or generic titles
      const shouldGenerateTitle =
        messages.length === 1 || // First exchange (only has user message)
        activeSession?.title === "New Chat" ||
        activeSession?.title?.includes(" - "); // Timestamp-based titles from project sessions

      if (shouldGenerateTitle) {
        // Generate title in background (don't block UI)
        generateChatTitle(
          userMessage.content,
          streamedContent || response.result,
          conversationFiles
        ).then(async (newTitle) => {
          try {
            await invoke("update_chat_session_title", {
              sessionId: chatSessionId,
              newTitle,
            });
            // Update local state
            setActiveSession(prev => ({ ...prev, title: newTitle }));
            setSessions(prev => prev.map(s => s.id === chatSessionId ? { ...s, title: newTitle } : s));
          } catch (err) {
            console.error("Failed to update chat title:", err);
          }
        });
      }
    } catch (err) {
      console.error("Failed to send message:", err);

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
    }
  };


  const handleKeyDown = (e) => {
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

  // Filter sessions based on search
  const filteredSessions = sessions.filter(session =>
    session.title?.toLowerCase().includes(chatFilter.toLowerCase())
  );

  return (
    <div className="h-full bg-white flex flex-col relative">
        {/* Header */}
        <div className="px-6 py-4 border-b border-gray-200 bg-white flex items-center justify-between">
          <div className="flex items-center gap-3">
            <svg
              className="w-6 h-6 text-blue-600"
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
            <div>
              <h2 className="text-lg font-semibold text-gray-900">Claude Code</h2>
              {focusedFile && (
                <p className="text-xs text-gray-500">
                  Focused: <span className="font-mono">{focusedFile.name}</span>
                </p>
              )}
            </div>

            {/* Model Selector */}
            <div className="relative ml-2">
              <button
                onClick={() => setShowModelSelector(!showModelSelector)}
                className="px-3 py-1.5 text-xs font-medium text-gray-700 bg-gray-100 hover:bg-gray-200 rounded transition-colors flex items-center gap-1.5"
                title="Change model"
              >
                <span>{selectedModel}</span>
                <svg className="w-3 h-3" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                  <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M19 9l-7 7-7-7" />
                </svg>
              </button>

              {showModelSelector && (
                <div className="absolute top-full left-0 mt-1 bg-white border border-gray-200 rounded-lg shadow-lg z-50 min-w-[160px]">
                  <div className="py-1">
                    {["haiku", "sonnet", "opus"].map((model) => (
                      <button
                        key={model}
                        onClick={() => {
                          setSelectedModel(model);
                          setShowModelSelector(false);
                        }}
                        className={`w-full px-4 py-2 text-left text-sm hover:bg-gray-100 transition-colors ${
                          selectedModel === model ? "bg-blue-50 text-blue-700 font-medium" : "text-gray-700"
                        }`}
                      >
                        {model.charAt(0).toUpperCase() + model.slice(1)}
                        {selectedModel === model && (
                          <span className="ml-2 text-blue-600">✓</span>
                        )}
                      </button>
                    ))}
                  </div>
                </div>
              )}
            </div>
          </div>
          <div className="flex items-center gap-2">
            {claudeInstalled && claudeInstalled.version && (
              <span className="text-xs text-gray-500">
                v{claudeInstalled.version}
              </span>
            )}
            <button
              onClick={createNewSession}
              className="px-3 py-1.5 text-xs font-medium text-blue-600 bg-blue-50 hover:bg-blue-100 rounded transition-colors flex items-center gap-1"
              title="Start a new chat"
            >
              <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M12 4v16m8-8H4" />
              </svg>
              <span>New Chat</span>
            </button>
            <button
              onClick={() => setShowHistory(!showHistory)}
              className="px-3 py-1.5 text-xs font-medium text-gray-700 bg-white border border-gray-300 rounded hover:bg-gray-50 transition-colors"
              title="View chat history"
            >
              {showHistory ? "Hide" : "History"}
            </button>
          </div>
        </div>

        {/* History Sidebar */}
        {showHistory && (
          <>
            {/* Overlay */}
            <div
              className="fixed inset-0 bg-black bg-opacity-20 z-40"
              onClick={() => setShowHistory(false)}
            />
            {/* Sidebar */}
            <div className="fixed left-0 top-0 bottom-0 w-80 bg-white border-r border-gray-200 shadow-lg z-50 flex flex-col">
              {/* Sidebar Header */}
              <div className="px-4 py-4 border-b border-gray-200">
                <div className="flex items-center justify-between mb-3">
                  <h3 className="text-lg font-semibold text-gray-900">Chat History</h3>
                  <button
                    onClick={() => setShowHistory(false)}
                    className="p-1 hover:bg-gray-100 rounded transition-colors"
                    title="Close"
                  >
                    <svg className="w-5 h-5 text-gray-600" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                      <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M6 18L18 6M6 6l12 12" />
                    </svg>
                  </button>
                </div>
                {/* Search Filter */}
                <input
                  type="text"
                  placeholder="Search chats..."
                  value={chatFilter}
                  onChange={(e) => setChatFilter(e.target.value)}
                  className="w-full px-3 py-2 text-sm border border-gray-300 rounded-lg focus:outline-none focus:ring-2 focus:ring-blue-500"
                />
              </div>

              {/* Chat List */}
              <div className="flex-1 overflow-y-auto">
                {loading ? (
                  <div className="px-4 py-8 text-center text-sm text-gray-500">Loading...</div>
                ) : filteredSessions.length > 0 ? (
                  <div className="py-2">
                    {filteredSessions.map((session) => (
                      <div
                        key={session.id}
                        className={`px-4 py-3 cursor-pointer hover:bg-gray-50 transition-colors flex items-center justify-between group border-l-2 ${
                          activeSession?.id === session.id
                            ? "bg-blue-50 border-blue-600"
                            : "border-transparent"
                        }`}
                        onClick={() => {
                          loadSession(session.id);
                          setShowHistory(false);
                        }}
                      >
                        <div className="flex-1 min-w-0">
                          <p className="text-sm font-medium text-gray-900 truncate">
                            {session.title || "Untitled Chat"}
                          </p>
                          {session.updated_at && (
                            <p className="text-xs text-gray-500 mt-1">
                              {new Date(session.updated_at).toLocaleDateString()}
                            </p>
                          )}
                        </div>
                        <button
                          onClick={(e) => {
                            e.stopPropagation();
                            deleteSession(session.id);
                          }}
                          className="opacity-0 group-hover:opacity-100 p-1.5 hover:bg-red-50 rounded transition-all ml-2"
                          title="Delete chat"
                        >
                          <svg
                            className="w-4 h-4 text-red-600"
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
                ) : chatFilter ? (
                  <div className="px-4 py-8 text-center text-sm text-gray-500">
                    No chats matching "{chatFilter}"
                  </div>
                ) : (
                  <div className="px-4 py-8 text-center text-sm text-gray-500">
                    No chat history yet
                  </div>
                )}
              </div>
            </div>
          </>
        )}

        {/* Claude Not Installed State */}
        {claudeInstalled && !claudeInstalled.installed ? (
          <div className="flex-1 flex items-center justify-center px-6 py-12">
            <div className="text-center max-w-md">
              <svg
                className="w-20 h-20 mx-auto mb-6 text-amber-400"
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
              <h3 className="text-xl font-semibold text-gray-900 mb-3">
                Claude Code CLI Not Installed
              </h3>
              <p className="text-sm text-gray-600 mb-6">
                To use the AI assistant, please install Claude Code CLI first
              </p>
              <a
                href="https://claude.com/claude-code"
                target="_blank"
                rel="noopener noreferrer"
                className="inline-block px-6 py-3 bg-blue-600 text-white rounded-lg hover:bg-blue-700 transition-colors font-medium"
              >
                Install Claude Code
              </a>
            </div>
          </div>
        ) : !aiEnabled ? (
          <div className="flex-1 flex items-center justify-center px-6 py-12">
            <div className="text-center max-w-md">
              <svg
                className="w-20 h-20 mx-auto mb-6 text-gray-300"
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
              <h3 className="text-xl font-semibold text-gray-900 mb-3">
                AI Features Disabled
              </h3>
              <p className="text-sm text-gray-600 mb-6">
                Enable AI features in Settings to use the AI assistant
              </p>
              <button
                onClick={onOpenSettings}
                className="px-6 py-3 bg-blue-600 text-white rounded-lg hover:bg-blue-700 transition-colors font-medium"
              >
                Open Settings
              </button>
            </div>
          </div>
        ) : (
          <>
            {/* Messages */}
            <div className="flex-1 overflow-y-auto px-6 py-6 space-y-6">
              {messages.length === 0 ? (
                <div className="text-center py-20 text-gray-400">
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
                      d="M8 10h.01M12 10h.01M16 10h.01M9 16H5a2 2 0 01-2-2V6a2 2 0 012-2h14a2 2 0 012 2v8a2 2 0 01-2 2h-5l-5 5v-5z"
                    />
                  </svg>
                  <p className="text-lg mb-2">Ask Claude Code to help with your project</p>
                  <p className="text-sm">Code analysis, debugging, file operations, and more</p>
                  {focusedFile && (
                    <p className="text-sm mt-4 text-blue-600">
                      Currently focused on: <span className="font-mono">{focusedFile.name}</span>
                    </p>
                  )}
                </div>
              ) : (
                messages.map((msg, idx) => (
                  <div
                    key={idx}
                    className={`flex ${
                      msg.role === "user" ? "justify-end" : msg.isStatus ? "justify-center" : "justify-start"
                    }`}
                  >
                    <div
                      className={`${
                        msg.isStatus
                          ? "px-4 py-2 rounded-full text-xs font-medium bg-blue-50 text-blue-700 border border-blue-200"
                          : `max-w-[80%] rounded-lg px-4 py-3 ${
                              msg.role === "user"
                                ? "bg-blue-600 text-white"
                                : msg.isError
                                ? "bg-red-50 text-red-800 border border-red-200"
                                : "bg-gray-100 text-gray-800"
                            }`
                      }`}
                    >
                      {/* Status messages (simple, centered) */}
                      {msg.isStatus ? (
                        <span>{msg.content}</span>
                      ) : (
                        <>
                          {/* Show progress events */}
                          {msg.progress && msg.progress.length > 0 && (
                            <div className="mb-3 space-y-2 p-3 bg-blue-50 rounded-lg border border-blue-200">
                              {msg.progress.map((progress, pIdx) => (
                                <div key={pIdx} className="flex items-center gap-2 text-xs text-blue-800 font-medium">
                                  <div className="w-1.5 h-1.5 bg-blue-600 rounded-full animate-pulse"></div>
                                  <span>{progress}</span>
                                </div>
                              ))}
                            </div>
                          )}

                          {msg.role === "user" ? (
                            <div className="whitespace-pre-wrap text-sm">
                              {msg.content}
                            </div>
                          ) : (
                            <div className="prose prose-sm max-w-none prose-headings:mt-3 prose-headings:mb-2 prose-p:my-2 prose-pre:bg-gray-800 prose-pre:text-gray-100 prose-code:text-blue-600 prose-code:bg-gray-100 prose-code:px-1 prose-code:py-0.5 prose-code:rounded">
                              {msg.isStreaming && !msg.content && msg.progress?.length === 0 ? (
                                // Show thinking indicator for streaming messages with no content yet
                                <div className="flex items-center gap-3 py-2">
                                  <div className="flex gap-1">
                                    <div className="w-2 h-2 bg-blue-600 rounded-full animate-bounce" style={{ animationDelay: "0ms" }}></div>
                                    <div className="w-2 h-2 bg-blue-600 rounded-full animate-bounce" style={{ animationDelay: "150ms" }}></div>
                                    <div className="w-2 h-2 bg-blue-600 rounded-full animate-bounce" style={{ animationDelay: "300ms" }}></div>
                                  </div>
                                  <span className="text-xs font-medium text-blue-800">Claude is thinking...</span>
                                </div>
                              ) : (
                                <>
                                  <ReactMarkdown remarkPlugins={[remarkGfm]}>
                                    {msg.content}
                                  </ReactMarkdown>
                                  {msg.isStreaming && msg.content && (
                                    <span className="inline-block w-2 h-4 ml-1 bg-gray-800 animate-pulse"></span>
                                  )}
                                </>
                              )}
                            </div>
                          )}
                        </>
                      )}
                    </div>
                  </div>
                ))
              )}

              {sending && messages[messages.length - 1]?.role !== "assistant" && (
                <div className="flex justify-start items-center gap-3">
                  <div className="bg-blue-50 text-blue-800 rounded-lg px-4 py-3 border border-blue-200 shadow-sm">
                    <div className="flex items-center gap-3">
                      <div className="flex gap-1">
                        <div className="w-2 h-2 bg-blue-600 rounded-full animate-bounce" style={{ animationDelay: "0ms" }}></div>
                        <div className="w-2 h-2 bg-blue-600 rounded-full animate-bounce" style={{ animationDelay: "150ms" }}></div>
                        <div className="w-2 h-2 bg-blue-600 rounded-full animate-bounce" style={{ animationDelay: "300ms" }}></div>
                      </div>
                      <span className="text-xs font-medium">Claude is thinking...</span>
                    </div>
                  </div>
                </div>
              )}

              <div ref={messagesEndRef} />
            </div>

            {/* Input */}
            <div className="border-t border-gray-200 bg-white px-6 py-4">
              <div className="flex gap-3">
                <textarea
                  ref={inputRef}
                  value={inputMessage}
                  onChange={(e) => setInputMessage(e.target.value)}
                  onKeyDown={(e) => {
                    handleKeyDown(e);
                    if (e.key !== "Enter") {
                      e.stopPropagation();
                    }
                  }}
                  placeholder={focusedFile ? `Ask about ${focusedFile.name}...` : "Ask Claude Code..."}
                  className="flex-1 px-4 py-3 border border-gray-300 rounded-lg resize-none focus:outline-none focus:ring-2 focus:ring-blue-500"
                  rows={3}
                  disabled={sending}
                />
                <button
                  onClick={handleSend}
                  disabled={!inputMessage.trim() || sending}
                  className="px-6 bg-blue-600 text-white rounded-lg hover:bg-blue-700 disabled:opacity-50 disabled:cursor-not-allowed transition-colors self-end"
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
  );
}
