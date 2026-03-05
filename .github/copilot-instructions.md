# Cortex-Works — Agent Rules

## Workspace Structure

```
crates/
  cortex-ast/     # Tree-sitter AST analysis
  cortex-act/     # File mutations + shell execution
  cortex-sync/    # Chat history + vector DB (LanceDB)
  cortex-db/      # Shared DB connection pool
  cortex-mcp/     # MCP gateway binary (IDE entry point)
  cortex-proxy/   # Local GGUF LLM HTTP proxy (llama-cpp-2)
  cortex-mesh/    # Cross-project IPC router
  cortex-scout/   # Web fetch / search / deep-research
apps/
  cortex-studio/  # React + Tauri v2 dashboard
  cortex-daemon/  # System tray: manages cortex-sync + cortex-proxy
apps/cortex-chat-ext/   # VS Code extension — Thai→English compression + Copilot injection
  extension.js          # Main logic: compress prompt → inject into Copilot
  release.sh            # Auto bump version + build + install  (./release.sh)
  install-local.sh      # Build + install to all detected IDEs
```

## Tool Map

| Task | Tool |
|------|------|
| Read / explore code | `cortex_code_explorer`, `cortex_symbol_analyzer` |
| Edit Rust/JS (by symbol) | `cortex_act_edit_ast` — never by line number |
| File write / patch / delete | `cortex_fs_manage` (actions: `write`, `patch`, `mkdir`, `delete`, `rename`, `move`, `copy`) |
| Edit JSON / YAML / TOML | `cortex_act_edit_data_graph` |
| Edit Markdown / HTML / XML | `cortex_act_edit_markup` |
| SQL schema changes | `cortex_act_sql_surgery` |
| Short commands / builds | `cortex_act_shell_exec` — always `run_diagnostics=true` for Rust builds |
| Long-running jobs | `cortex_job_manager` (actions: `start`, `poll`, `kill`) |
| Batch independent ops | `cortex_act_batch_execute` |
| Recall past decisions | `cortex_memory_retriever` |
| Reload MCP worker | `cortex_mcp_hot_reload` |
| Save before risky edits | `cortex_save_checkpoint` |

### Removed shims — never use

| ~~Old~~ | Use instead |
|---------|-------------|
| ~~cortex_write_file~~ | `cortex_fs_manage(action=write)` |
| ~~cortex_patch_file~~ | `cortex_fs_manage(action=patch)` |
| ~~cortex_act_run_async~~ | `cortex_job_manager(action=start)` |
| ~~cortex_check_job~~ | `cortex_job_manager(action=poll)` |
| ~~cortex_kill_job~~ | `cortex_job_manager(action=kill)` |
| ~~cortex_mcp_refresh_tools~~ | `cortex_mcp_hot_reload` |

## Workflow

```
1. cortex_memory_retriever        — recall context
2. cortex_code_explorer           — map structure (skip if recent)
3. cortex_save_checkpoint         — before any destructive/risky edit
4. edit via correct tool above
5. cortex_act_shell_exec          — verify: cargo build / diagnostics
```

## Rules

- **Edit by symbol, not line.** Use `cortex_act_edit_ast` with symbol names.
- **Batch reads.** Combine independent reads in one `cortex_act_batch_execute`.
- **Always verify.** After any Rust change: `cargo build -p <crate>` with `run_diagnostics=true`.
- **No explanations of tools.** Just execute — skip preamble like "I will now use...".
- **Fail fast.** If blocked, state the exact constraint immediately.

## Running the Stack

```bash
# Full stack (proxy embedded):
cargo run --release -p cortex-mcp

# Proxy only (standalone daemon):
cargo run --release -p cortex-proxy

# Rebuild + deploy cortex-proxy after source changes:
cargo build --release -p cortex-proxy
cp target/release/cortex-proxy ~/.cortex/bin/cortex-proxy
codesign -s - --force ~/.cortex/bin/cortex-proxy
kill $(pgrep -f cortex-proxy) && ~/.cortex/bin/cortex-proxy &

# Tray daemon (dev):
cargo tauri dev --manifest-path apps/cortex-daemon/Cargo.toml
```

## cortex-chat-ext

- Compression prompt style: **Minimal Translator** — direct translation, no semantic extraction.
- System: `"You are a translator."` | Post-prompted RULES after TEXT block.
- Release: `cd apps/cortex-chat-ext && ./release.sh` (auto-bumps patch, builds, installs).
- Flags: `--minor`, `--major`, `--build-only`, `--no-bump`.
- Port scan: `8080–8089` for local proxy endpoint.

## LLM Proxy

- Binary: `~/.cortex/bin/cortex-proxy`  |  Port: `8080–8089` (env `CORTEX_PROXY_PORT`)
- Endpoint: `POST /v1/local/chat/completions`
- Model dir: `~/.cortex/models/`  |  GPU-first (Metal/CUDA) → CPU fallback
- `LlamaBatch` sized to `tokens.len().max(512)` — do not hardcode 1024.
