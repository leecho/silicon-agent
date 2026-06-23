# Silicon Agent

English | [中文](README.md)

Silicon Agent is an open-source AI agent desktop client. Packaged with Tauri 2,
it uses the system WebView and a Rust backend instead of bundling Chromium, so
its installer size and runtime footprint are typically far smaller than an
Electron app. At its core is a single-session agent loop (multi-turn reasoning,
streaming, tool calls, auto-compaction) backed by your own OpenAI-compatible
model provider, paired with a focused set of built-in tools, file-based skills,
and optional access from IM channels. The goal is a low-cost, self-hosted agent
you can run locally and point at your own machine and files.

## Features

- **Agent chat sessions** — a single-session agent loop with streaming
  responses, tool calls, interrupt/stop, and auto-compaction.
- **Built-in tools** — file read / write / edit, `glob`, `grep`,
  `run_command`, `web_search`, `web_fetch`, plus interaction/flow tools
  (`ask_user`, `update_todos`, `propose_plan`, `add_artifact`).
- **File-based skills** — load, install, and read `SKILL.md` skills from
  `~/.siliconagent/skills/`; built-in skills ship with the app and are
  materialized on startup.
- **Provider & model settings** — configure your own OpenAI-compatible API
  endpoint, keys, and models in-app.
- **IM remote channels** — route messages from IM channels into ordinary
  agent sessions.
- **Observability** — usage statistics and a model call log for inspecting
  requests, tokens, and costs.

## Not included

Silicon Agent is intentionally a focused core. It does **not** include
multi-agent teams or experts, sub-agent dispatch, project workspaces,
plugins/suites, MCP connectors, scheduled tasks, or long-term memory. If you
need those, see **Silicon Worker** below.

## Tech stack

Tauri 2 · Rust (edition 2021) · React 18 + TypeScript 5 · Vite 5 · Tailwind 3 ·
SQLite (rusqlite, bundled)

## Development

The packaged app requires no Python or external runtime. For local development
you need Rust (with cargo) and Node.js.

Backend tests:

```bash
cargo test --manifest-path src-tauri/Cargo.toml
```

Frontend build:

```bash
npm run build
```

Run the desktop app in dev (opens a window):

```bash
npm run tauri:dev
```

## About Silicon Worker

Silicon Agent is the focused open-source core. If you need a fuller feature set,
take a look at its sibling, **Silicon Worker** — a full-featured AI agent desktop
client for local work. On top of everything in Silicon Agent, Silicon Worker also
provides:

- Multi-agent collaboration and sub-agents (`dispatch_agent`) with serial /
  parallel scheduling
- Expert and team collaboration catalogs
- A long-lived agent workbench and agent self-evolution (SOUL version history)
- Project workspaces, task boards, and project-level instructions
- Plugins and MCP (stdio / http) extensions
- A memory system (user profile, long-term facts, project / session memory)
- Scheduled tasks and remote channels such as WeChat, DingTalk, Feishu, and
  Telegram
- Finer-grained usage and audit analytics

Project and releases: <https://github.com/leecho/silicon-worker-release>

## License

Licensed under the GNU Affero General Public License v3.0 or later
(AGPL-3.0-or-later). See [LICENSE](LICENSE).
