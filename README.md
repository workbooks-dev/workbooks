# Tether

*Sharpen your automations*

Notebooks as automations. Built for speed, security, privacy, open source, and desktop-first. Use our AI Agent to debug and build.

## What It Does

Run Jupyter notebooks on schedule with built-in secrets management and AI-powered assistance.

**Key Features:**
- **Scheduled Execution** - Run notebooks daily, hourly, weekly, or custom cron schedules
- **Secrets Management** - Touch ID encrypted secrets, automatic kernel injection
- **AI Agent Integration** - Natural language debugging and notebook building
- **Local-First** - Everything runs on your machine, no cloud required
- **Fast & Secure** - Native desktop app built with Tauri (Rust + React)
- **Open Source** - Full access to code, inspect and modify as needed

## Perfect For

- Data analysts who want reliable automation without cloud dependencies
- Researchers running scheduled data collection and analysis
- Teams who need privacy-first automation solutions
- Anyone who wants to build automations through natural language

## Quick Start

```bash
# Clone the repository
git clone https://github.com/yourusername/tether.git
cd tether

# Install dependencies
npm install

# Run in development mode
npm run tauri dev

# Build for production
npm run tauri build
```

## Tech Stack

- **Tauri** - Native desktop app framework (Rust + React)
- **Agent SDK** - AI-powered notebook building and debugging
- **Jupyter** - Python execution engine (AsyncKernelManager)
- **UV** - Bundled Python environment manager
- **Monaco Editor** - Code editing and inspection
- **SQLite** - Local storage for schedules, runs, and secrets

## Documentation

All feature documentation is in the `features/` directory:
- `features/<area>/docs.md` - Feature design and how it works
- `features/<area>/todo.md` - What needs to be implemented
- `features/<area>/done.md` - What's been completed

See `CLAUDE.md` for full development guide.

## License

[Add your license here]

## Contributing

[Add contributing guidelines here]
