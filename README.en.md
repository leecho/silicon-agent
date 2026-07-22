<div align="center">

# Silicon Worker · 硅基动力

**A local-first AI Agent desktop client built around one goal: minimal install, config, authorization, and day-to-day cost.**

Tauri 2 · Rust · React 18 + TypeScript · Vite · Tailwind · local SQLite

[Website www.silicower.com](https://www.silicower.com) · [中文](README.md) · [Releasing](RELEASING.md) · [Contributing](CONTRIBUTING.md)

</div>

---

## What it is

silicon-worker is an **install-and-go, data-stays-on-your-machine** AI Agent desktop app. You configure your own model in-app (any OpenAI-compatible provider, or the native Anthropic API), and it autonomously drives tools in a multi-turn ReAct loop to get work done — reading/writing files, running commands, searching the web, operating a browser and the desktop — all on your computer, under your authorization.

It currently ships as a "Tauri shell + Rust runtime + local SQLite + on-machine tools" — not because "local desktop" is the goal itself, but because that path lets users **deploy less, configure less, and depend on fewer external services**, while safely operating their own computer. The packaged app **does not require you to install Python or Rust**.

> **Core product principle: don't replace model-driven agent behavior with brittle business keyword rules.**
> Natural-language intent, task planning, skill evolution, memory extraction, and workflow adaptation are model-driven and schema-validated; only high-confidence structured signals use deterministic rules.

## Screenshots

<div align="center">

**Home — just say what you want done, pick an expert / team to do it**

<img src="screenshots/home.png" alt="silicon-worker home" width="820">

**Multi-expert team collaboration — a live monitor panel of tasks and member output**

<img src="screenshots/team-roundtable.png" alt="multi-expert team collaboration" width="820">

**Capability market — one-click install of shared skills / experts / teams / plugins**

<img src="screenshots/market.png" alt="capability market" width="820">

**Capability hub — enable, group, and AI-create / install skills**

<img src="screenshots/skills.png" alt="capability hub - skills" width="820">

**Project task board — instructions / tasks / artifacts / memory / knowledge in one place**

<img src="screenshots/project-kanban.png" alt="project task board" width="820">

**Runtime settings — context compaction, iteration caps, sub-agents, and retry**

<img src="screenshots/settings.png" alt="runtime settings" width="820">

</div>

## Highlights

### 🤖 Autonomous agent engine
- **Multi-turn ReAct**: call model → batch-execute tools → observe → continue, with empty-response/truncation rescue and unfinished-todo nudging.
- **Resume & interrupt**: runs execute on a detached thread, so refreshing or reopening the app never interrupts in-flight output; stop anytime with one click.
- **Multi-session isolation**: each session is its own conversation line with independent history, working directory, and run state.
- **One-shot authorization & clarification**: reads run directly, writes/dangerous ops ask for consent per your permission mode (one grant per identical args); `ask_user` asks you structured follow-ups.

### 🔌 Multi-provider LLM
- **OpenAI-compatible**: DeepSeek / Azure / DashScope / Ollama / OpenRouter, and more.
- **Native Anthropic**: Claude via `/v1/messages`, with streaming, tool calls, thinking display, and cache-token accounting aligned to the OpenAI path.
- Primary/secondary model routing with fallback, plus an auxiliary model for titles, suggestions, and message enhancement.

### 🛠️ Tools & sandbox
- Built-in file read/write/search, command execution, web fetch & search, knowledge retrieval, sandboxed to the workspace root.
- Concurrency-safe tools batch in parallel; non-concurrency-safe ones run serially.
- **Desktop control** (read on-screen UI elements, then click/type/scroll) and **Browser control** (a dedicated automation browser window with login state reused across sessions) — both driven by plain-text UI structure, so **any model works, including non-multimodal ones like DeepSeek**.
- **Apple tools** (macOS): Calendar, Reminders (EventKit), Notes (automation), with high-risk actions gated by your permission mode.

### 🧩 Capability system: Skills / Experts / Teams / Plugins
- **Skills**: file-based `SKILL.md` (YAML frontmatter); built-ins are embedded and materialized on startup; install via drag-drop / zip / directory, toggle, or remove.
- **Experts**: editable capability templates that can **seed** an **Agent** with its own **private long-term memory** that remembers you across sessions.
- **Agent self-evolution**: after accumulating experience, an agent reflects and **proposes** rewrites to its personality (SOUL) that take effect only on your approval; the identity anchor (IDENTITY) is never auto-changed and versions are revertible.
- **Teams**: session-level multi-member orchestration (lead + members).
- **Plugins**: an entry point into the standard ecosystem (Claude / Codex conventions) where skill/agent/command/hook/mcpServers are global and public.

### 🏪 Capability markets
- Add **static repo URLs** hosted on GitHub / Gitee under Settings → Market sources (zero hosting cost) and add others' shared skills / experts / teams with one click.
- Aggregated by source with provenance labels; third-party sources ask consent at both add and install time; markets can be enabled / disabled / removed anytime.
- Export your own experts / teams into market-repo format to run your own market.

### 🧠 Memory, knowledge & observability
- **Long-term memory** so an agent remembers you across sessions.
- **Knowledge bases** with optional vector retrieval and configurable embedding model.
- **Usage analytics** by date / model / session / hour, including cache hit/write and hit rate.
- **Call logs** (optional): full request/response/token/latency per model call, covering the main session, sub-agents, title/suggestion generation, context compaction, and memory maintenance.

### 🔗 Connectivity & automation
- **MCP**: Model Context Protocol external tools (including standard OAuth auth-code + PKCE).
- **Scheduler**: cron-style triggers for agents.
- **Remote access**: bind WeChat (ClawBot) by QR code for two-way chat with your local agent — assign tasks, receive replies, approve risky ops / answer follow-ups / approve plans by number, while tools still run locally.
- **Artifacts**: agents register deliverable files via `add_artifact`, shown inline and previewable.

## Tech stack

| Layer | Tech |
| --- | --- |
| Frontend | React 18 · TypeScript 5 · Vite 5 · Tailwind CSS 3 |
| Desktop shell | Tauri 2 |
| Backend / runtime | Rust (edition 2021), blocking + threads |
| Persistence | local SQLite in the app data dir (rusqlite bundled) |
| LLM | your in-app OpenAI-compatible / native Anthropic API |
| Skills | local `SKILL.md` files under `~/.siliconworker/skills/` |

## Quick start (development)

### Prerequisites
- [Rust](https://rustup.rs/) toolchain (edition 2021)
- Node.js (24.x recommended)
- Tauri 2 system dependencies — see [Tauri prerequisites](https://tauri.app/start/prerequisites/)
- On macOS, grant the relevant permissions when prompted for desktop / browser / Apple tools

```bash
npm install            # install deps
npm run tauri:dev      # local dev (opens the desktop window)
npm run build          # frontend build only (tsc + vite, no lint)
cargo test --manifest-path src-tauri/Cargo.toml   # backend tests
npm run tauri:build    # package the desktop app
```

> You don't need a full package build on every change — it's slow. See [RELEASING.md](RELEASING.md) for the release flow.

## Project layout

```
silicon-worker/
├── src/                 # Frontend (React + TS): pages/, components/, hooks/, lib/, api/
├── src-tauri/           # Backend (Rust + Tauri)
│   ├── src/
│   │   ├── engine/      # ReAct engine / runner
│   │   ├── provider/    # LLM provider abstraction
│   │   ├── tools/       # built-in tools + registry + sandbox
│   │   ├── skill/ expert/ team/ plugin/    # capability system
│   │   ├── agent/ memory/ scheduler/       # agents / memory / scheduling
│   │   ├── mcp/ browser/ desktop/ apple/   # connectivity & on-machine control
│   │   ├── market/ remote/ knowledge/      # markets / remote / knowledge
│   │   ├── commands/    # Tauri command layer
│   │   └── storage/     # SQLite
│   └── builtin-skills/  # bundled skills (xlsx/pdf/docx/pptx/create-* ...)
├── screenshots/         # UI preview images
├── CONTRIBUTING.md      # contributing guide & CLA
└── RELEASING.md         # release notes
```

## Security & privacy

- **Data stays local**: sessions, memory, and settings live in local SQLite in the app data dir; the model API is yours to configure.
- **Destructive ops require confirmation**: overwriting/deleting files, bulk move/rename, reading sensitive local files, connector ops touching external services.
- **Atomic file tools avoid overwrite by default**, preferring safe write / copy / rename.
- **Remote access is outbound-only**: the WeChat channel uses long polling with no public relay; only allowlisted peers can drive it.

## Contributing

Issues and PRs welcome. See [CONTRIBUTING.md](CONTRIBUTING.md) for how to contribute, pre-submit checks, and the Contributor License Agreement (CLA). Run the relevant checks before submitting (`cargo test` for backend, `npm run build` for frontend).

## License

This project is licensed under the **[PolyForm Noncommercial License 1.0.0](LICENSE)** — a **source-available, noncommercial-only** license:

- ✅ Anyone may **view, study, modify, and distribute** the source for research, learning, personal, and nonprofit purposes.
- 🚫 **Commercial use is not permitted.** For commercial use, contact the author for a separate commercial license.
- 🏷️ Copyright is held by the author (`leecho · leecho571@gmail.com`), who reserves all rights to commercialize this project.

> Note: PolyForm Noncommercial is a *source-available* license, not an OSI-approved *open-source* license — it trades commercial use for the protections above.
> To use this project commercially, reach out by email. See [CONTRIBUTING.md](CONTRIBUTING.md) for how to contribute and the CLA.

---

<div align="center">
<sub>Silicon Worker · 硅基动力 · <a href="https://www.silicower.com">www.silicower.com</a> · install-and-go agents, data on your machine</sub>
</div>
