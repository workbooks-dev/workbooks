# AI Assistant - Todo

## Critical Issues

**All critical issues have been resolved!**

- [x] **Notebook change visibility and approval** (COMPLETED - Dec 27, 2025)
  - Implemented NotebookDiffModal component with cell-by-cell diff view
  - Shows color-coded changes (green=added, blue=modified, red=deleted)
  - Side-by-side before/after comparison for modified cells
  - Approve/reject buttons with proper workflow
  - Version history system already exists in Rust backend
  - Works like Cursor/Windsurf/Antigravity with visual diffs
  - See: src/components/NotebookDiffModal.jsx, App.jsx, features/ai-assistant/done.md

## High Priority

- [ ] Test AI-first interface with real Claude Code CLI
- [x] Project-level context injection (DONE - automatic notebook scanning and context)
- [ ] Ensure focused file context works correctly in prompts
- [ ] Verify split-view layout works on different screen sizes
- [ ] Add keyboard shortcut to toggle file viewer (Cmd+B?)
- [x] Add Markdown rendering for chat UI (code blocks, formatting) (DONE - using react-markdown with Tailwind typography)
- [ ] Consider adding file contents preview in AI chat when focused
- [ ] Reload project context when notebooks are added/removed (file watcher integration)

## Medium Priority

- [x] Show tool use in chat (DONE - progress indicators show Read, Edit, Bash, etc. with improved visibility)
- [x] Prominent "New Chat" button (DONE - always visible in header)
- [ ] Add "copy to clipboard" button for code blocks in responses
- [x] Implement markdown rendering for agent responses (DONE - full GFM support with styled code blocks)
- [ ] Add "clear conversation" button

## Low Priority

- [ ] Add keyboard shortcuts (Cmd+K to open, Esc to close)
- [ ] Implement session export (markdown/PDF)
- [ ] Add dark mode support for chat UI
- [ ] Add message edit/delete functionality
- [ ] Implement multi-modal support (images, files)

## Future Enhancements

- [ ] Voice input support
- [ ] Inline code suggestions in workbook editor
- [ ] Agent templates/personas
- [ ] Multi-project chat history
- [ ] Collaborative sessions (team chat)
- [ ] Agent memory and context persistence
