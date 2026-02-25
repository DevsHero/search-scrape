# 🥷 Cortex Scout MCP — v3.1.0

<div align="center">
<img src="media/logo.svg" alt="Cortex Scout Logo" width="180">
<h3><b>Search Smarter. Scrape Anything. Block Nothing.</b></h3>
<p><b>The God-Tier Intelligence Engine for AI Agents</b></p>
<p><i>The Sovereign, Self-Hosted Alternative to Firecrawl, Jina, and Tavily.</i></p>

[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)
[![Rust](https://img.shields.io/badge/Built%20with-Rust-orange.svg)](https://www.rust-lang.org/)
[![MCP](https://img.shields.io/badge/Protocol-MCP-blue.svg)](https://modelcontextprotocol.io/)
[![CI](https://github.com/cortex-works/cortex-scout/actions/workflows/ci.yml/badge.svg)](https://github.com/cortex-works/cortex-scout/actions/workflows/ci.yml)
</div>

---

**Cortex Scout** is not just a scraper or a search wrapper — it is a **complete intelligence layer** purpose-built for AI Agents. Cortex Scout ships a **native Rust meta-search engine** running inside the same binary. Zero extra containers. Parallel engines. LLM-grade clean output.

When every other tool gets blocked, Cortex Scout doesn't retreat — it **escalates**: native engines → native Chromium CDP headless → Human-In-The-Loop (HITL) nuclear option. You always get results.

---

## ⚡ God-Tier Internal Meta-Search (v3.0.0)

Cortex Scout v3.0.0 ships a **100% Rust-native metasearch engine** that queries 4 engines in parallel and fuses results intelligently:

| Engine | Coverage | Notes |
|--------|----------|-------|
| 🔵 **DuckDuckGo** | General Web | HTML scrape, no API key needed |
| 🟢 **Bing** | General + News | Best for current events |
| 🔴 **Google** | Authoritative Results | High-relevance, deduped |
| 🟠 **Brave Search** | Privacy-Focused | Independent index, low overlap |

### 🧠 What makes it God-Tier?

**Parallel Concurrency** — All 4 engines fire simultaneously. Total latency = slowest engine, not sum of all.

**Smart Deduplication + Scoring** — Cross-engine results are merged by URL fingerprint. Pages confirmed by 2+ engines receive a corroboration score boost. Domain authority weighting (docs, .gov, .edu, major outlets) pushes high-trust sources to the top.

**Ultra-Clean Output for LLMs** — Clean fields and predictable structure:
- `published_at` is parsed and stored as a clean **ISO-8601 field** (`2025-07-23T00:00:00`)
- `content` / `snippet` is clean — zero date-prefix garbage
- `breadcrumbs` extracted from URL path for navigation context
- `domain` and `source_type` auto-classified (`blog`, `docs`, `reddit`, `news`, etc.)

**Result: LLMs receive dense, token-efficient, structured data — not a wall of noisy text.**

**Unstoppable Fallback** — If an engine returns a bot-challenge page (`anomaly.js`, Cloudflare, PerimeterX), it is automatically retried via the native Chromium CDP instance (headless Chrome, bundled in-binary). No manual intervention. No 0-result failures.

**Quality > Quantity** — ~20 deduplicated, scored results rather than 50 raw duplicates. For an AI agent with a limited context window, 20 high-quality results outperform 50 noisy ones every time.

---

## � Deep Research Engine (v3.1.0)

Cortex Scout v3.1.0 ships a self-contained **multi-hop research pipeline** as a first-class MCP tool — no external infra, no key required for local LLMs.

### How it works

1. **Query Expansion** — expands your question into multiple targeted sub-queries (3 axes: core concept, comparison/alternatives, implementation specifics)
2. **Parallel Search + Scrape** — fires all sub-queries across 4 search engines; auto-scrapes top results (configurable depth 1–3, up to 20 sources)
3. **Semantic Filtering** — Model2Vec-powered relevance scoring keeps only on-topic content chunks
4. **LLM Synthesis** — condenses all findings into a zero-fluff Markdown fact-sheet via any OpenAI-compatible API

### LLM Backend Options

| Backend | `llm_base_url` | Key required |
|---------|---------------|-------------|
| **OpenAI** (default) | `https://api.openai.com/v1` | Yes — `OPENAI_API_KEY` |
| **Ollama** (local) | `http://localhost:11434/v1` | No |
| **LM Studio** (local) | `http://localhost:1234/v1` | No |
| Any OpenAI-compatible proxy | custom URL | Optional |

### Configuration (`cortex-scout.json`)

Create `cortex-scout.json` in the same directory as the binary (or repo root) to configure the engine — no rebuild needed. All fields are optional; env vars are used as fallback.

```json
{
  "deep_research": {
    "enabled": true,
    "llm_base_url": "http://localhost:11434/v1",
    "llm_api_key": "",
    "llm_model": "llama3",
    "synthesis_enabled": true,
    "synthesis_max_sources": 8,
    "synthesis_max_chars_per_source": 2500,
    "synthesis_max_tokens": 1024
  }
}
```

**Priority:** `cortex-scout.json` field → env var fallback → hardcoded default.

### Build flags

```bash
# Full build (deep_research included by default)
cargo build --release

# Lean build — strip deep_research feature entirely
cargo build --release --no-default-features --features non_robot_search
```

> The `deep-research` Cargo feature is **on by default**. Use `--no-default-features` for minimal deployments.

---

## �🛠 Full Feature Roster

| Feature | Details |
|---------|---------|
| � **Deep Research Engine** | Multi-hop search + scrape + semantic filter + LLM synthesis (OpenAI / Ollama / LM Studio) |
| �🔍 **God-Tier Meta-Search** | Parallel Google / Bing / DDG / Brave · dedup · scoring · breadcrumbs · `published_at` |
| 🕷 **Universal Scraper** | Rust-native + native Chromium CDP for JS-heavy and anti-bot sites |
| 🛂 **Human Auth (HITL)** | `human_auth_session`: Real browser + persistent cookies + instruction overlay + Automatic Re-injection. Fetch any protected URL. |
| 🧠 **Semantic Memory** | Embedded LanceDB + Model2Vec for long-term research recall (no DB container) |
| 🤖 **HITL Non-Robot Search** | Visible Brave Browser + keyboard hooks for human CAPTCHA / login-wall bypass |
| 🌐 **Deep Crawler** | Recursive, bounded crawl to map entire subdomains |
| 🔒 **Proxy Master** | Native HTTP/SOCKS5 pool rotation with health checks |
| 🧽 **Universal Janitor** | Strips cookie banners, popups, skeleton screens — delivers clean Markdown |
| 🔥 **Hydration Extractor** | Resolves React/Next.js hydration JSON (`__NEXT_DATA__`, embedded state) |
| 🛡 **Anti-Bot Arsenal** | Stealth UA rotation, fingerprint spoofing, CDP automation, mobile profile emulation |
| 📊 **Structured Extract** | CSS-selector + prompt-driven field extraction from any page |
| 🔁 **Batch Scrape** | Parallel scrape of N URLs with configurable concurrency |

---

## 🏗 Zero-Bloat Architecture

Cortex Scout is **pure binary**: a single Rust executable exposes MCP tools (stdio) and an optional HTTP server — no Docker, no sidecars.

---

## 💎 The Nuclear Option: Human Auth Session (v3.0.0)

When standard automation fails (Cloudflare, CAPTCHA, complex logins), Cortex Scout **activates the human element.**

### 🛂 `human_auth_session` — The "Unblocker"
This is our signature tool that surpasses all competitors. While most scrapers fail on login-walled content, `human_auth_session` opens a **real, visible browser window** for you to solve the challenge. 

Once you click **FINISH & RETURN**, all authentication cookies are transparently captured and persisted in `~/.cortex-scout/sessions/`. Subsequent requests to the same domain automatically inject these cookies — making future fetches **fully automated** and **effortless.**

- 🟢 **Instruction Overlay** — A native green banner guides the user on what to solve.
- 🍪 **Persistent Sessions** — Solve once, scrape forever. No need to log in manually again for weeks.
- 🛡 **Security first** — Cookies are stored locally and encrypted (optional/upcoming).
- 🚀 **Auto-injection** — Next `web_fetch` or `web_crawl` calls automatically load found sessions.

---

## 💥 Boss-Level Anti-Bot Evidence

We don't claim — we show receipts. All captured with `human_auth_session` and our advanced CDP engines (2026-02-20):

| Target | Protection | Evidence | Extracted |
|--------|-----------|----------|-----------|
| **LinkedIn** | Cloudflare + Auth | [JSON](proof/linkedin_evidence.json) · [Snippet](proof/linkedin_raw_snippet.txt) | 60+ job listings ✅ |
| **Ticketmaster** | Cloudflare Turnstile | [JSON](proof/ticketmaster_evidence.json) · [Snippet](proof/ticketmaster_raw_snippet.txt) | Tour dates & venues ✅ |
| **Airbnb** | DataDome | [JSON](proof/airbnb_evidence.json) · [Snippet](proof/airbnb_raw_snippet.txt) | 1,000+ Tokyo listings ✅ |
| **Upwork** | reCAPTCHA | [JSON](proof/upwork_evidence.json) · [Snippet](proof/upwork_raw_snippet.txt) | 160K+ job postings ✅ |
| **Amazon** | AWS Shield | [JSON](proof/amazon_evidence.json) · [Snippet](proof/amazon_raw_snippet.txt) | RTX 5070 Ti search results ✅ |
| **nowsecure.nl** | Cloudflare | [JSON](proof/nowsecure_evidence.json) | Manual button verified ✅ |

📖 Full analysis: [proof/README.md](proof/README.md)

---

## 📦 Quick Start

### Option A — Download Prebuilt Binaries (Recommended)

Download the latest release assets from GitHub Releases and run one of:

Prebuilt assets are published for: `windows-x64`, `windows-arm64`, `linux-x64`, `linux-arm64`.

- `cortex-scout-mcp` — MCP stdio server (recommended for VS Code / Cursor / Claude Desktop)
- `cortex-scout` — HTTP server (default port `5000`; override via `--port`, `PORT`, or `CORTEX_SCOUT_PORT`)

Confirm the HTTP server is alive:
```bash
./cortex-scout --port 5000
curl http://localhost:5000/health
```

---

## 🧪 Build (Release, All Features)

Build all binaries with all optional features enabled:

```bash
cd mcp-server
cargo build --release --all-features
```

---
### Option B — Build / Install from Source

```bash
git clone https://github.com/DevsHero/cortex-scout.git
cd cortex-scout
```

Build:
```bash
cd mcp-server
cargo build --release --features non_robot_search --bin cortex-scout --bin cortex-scout-mcp
```

Or install (puts binaries into your Cargo bin directory):
```bash
cargo install --path mcp-server --locked
```

Binaries land at:
- `target/release/cortex-scout` — HTTP server (default port `5000`; override via `--port`, `PORT`, or `CORTEX_SCOUT_PORT`)
- `target/release/cortex-scout-mcp` — MCP stdio server

Prerequisites for HITL:
- **Brave Browser** ([brave.com/download](https://brave.com/download/))
- **Accessibility permission** (macOS: System Preferences → Privacy & Security → Accessibility)
- A desktop session (not SSH-only)

Platform guides: [docs/window_setup.md](docs/window_setup.md) · [docs/ubuntu_setup.md](docs/ubuntu_setup.md)

> After any binary rebuild/update, **restart your MCP client session** to pick up new tool definitions.

---

## ✅ Agent Best Practices (Cortex Scout Rules)

Use this exact decision flow to get the highest-quality results with minimal tokens:

1) `memory_search` first (avoid re-fetching)
2) `web_search_json` for initial research (search + content summaries in one call)
3) `web_fetch` for specific URLs (docs/articles)
        - `output_format="clean_json"` for token-efficient output
        - set `query` + `strict_relevance=true` when you want only query-relevant paragraphs
4) If `web_fetch` returns 403/429/rate-limit → `proxy_control` `grab` then retry with `use_proxy=true`
5) If `web_fetch` returns `auth_risk_score >= 0.4` → `visual_scout` (confirm login wall) → `human_auth_session` (The God-Tier Nuclear Option)

Structured extraction (schema-first):
- Prefer `fetch_then_extract` for one-shot **fetch + extract**.
- `strict=true` (default) enforces schema shape: missing arrays become `[]`, missing scalars become `null` (no schema drift).
- Treat `confidence=0.0` as “placeholder / unrendered page” (often JS-only like crates.io). Escalate to browser rendering (CDP/HITL) instead of trusting the fields.
- 💡 **New in v3.0.0**: Placeholder detection is now **scalar-only**. Pure-array schemas (only lists/structs) never trigger confidence=0.0, fixing prior regressions.

`clean_json` notes:
- Large pages are truncated to respect `max_chars` (look for `clean_json_truncated` warning). Increase `max_chars` to see more.
- `key_code_blocks` is extracted from fenced blocks and signature-like inline code; short docs pages are supported.
- 🕷 **v3.0.0 fix**: Module extraction on `docs.rs` works recursively for all relative and absolute sub-paths.



---

## 🧩 MCP Integration

Cortex Scout exposes all tools via the **Model Context Protocol** (stdio transport).

### VS Code / Copilot Chat 

Add to your MCP config (`~/.config/Code/User/mcp.json`):

```jsonc
{
  "servers": {
    "cortex-scout": {
      "type": "stdio",
      "command": "env",
      "args": [
        "RUST_LOG=info",
        "SEARCH_ENGINES=google,bing,duckduckgo,brave",
        "LANCEDB_URI=/YOUR_PATH/cortex-scout/lancedb",
        "HTTP_TIMEOUT_SECS=30",
        "MAX_CONTENT_CHARS=10000",
        "IP_LIST_PATH=/YOUR_PATH/cortex-scout/ip.txt",
        "PROXY_SOURCE_PATH=/YOUR_PATH/cortex-scout/proxy_source.json",
        "/YOUR_PATH/cortex-scout/mcp-server/target/release/cortex-scout-mcp"
      ]
    }
  }
}
```

### Cursor / Claude Desktop

Use the same stdio setup as VS Code (run `cortex-scout-mcp` locally and pass env vars via `env` or your client’s `env` field).

📖 Full multi-IDE guide: [docs/IDE_SETUP.md](docs/IDE_SETUP.md)

---

## ⚙️ Key Environment Variables

| Variable | Default | Description |
|----------|---------|-------------|
| `CHROME_EXECUTABLE` | auto-detected | Override path to Chromium/Chrome/Brave binary |
| `SEARCH_ENGINES` | `google,bing,duckduckgo,brave` | Active search engines (comma-separated) |
| `SEARCH_MAX_RESULTS_PER_ENGINE` | `10` | Results per engine before merge |
| `SEARCH_CDP_FALLBACK` | `true` if browser found | Auto-retry blocked engines via native Chromium CDP (alias: `SEARCH_BROWSERLESS_FALLBACK`) |
| `SEARCH_SIMULATE_BLOCK` | — | Force blocked path for testing: `duckduckgo,bing` or `all` |
| `LANCEDB_URI` | — | Path for semantic research memory (optional) |
| `CORTEX_SCOUT_NEUROSIPHON` | `1` (enabled) | Set to `0` / `false` / `off` to disable all NeuroSiphon techniques (import nuking, SPA extraction, semantic shaving, search reranking) |
| `HTTP_TIMEOUT_SECS` | `30` | Per-request timeout |
| `OUTBOUND_LIMIT` | `32` | Max concurrent outbound connections |
| `MAX_CONTENT_CHARS` | `10000` | Max chars per scraped document |
| `IP_LIST_PATH` | — | Path to proxy IP list |
| `SCRAPE_DELAY_PRESET` | `polite` | `fast` / `polite` / `cautious` |
| `DEEP_RESEARCH_ENABLED` | `1` (enabled) | Set `0` to disable the `deep_research` tool at runtime (without rebuild) |
| `OPENAI_API_KEY` | — | API key for LLM synthesis. Leave unset for key-less local endpoints (Ollama / LM Studio) |
| `OPENAI_BASE_URL` | `https://api.openai.com/v1` | LLM endpoint. Override for Ollama (`http://localhost:11434/v1`) or LM Studio (`http://localhost:1234/v1`). Config: `deep_research.llm_base_url` |
| `DEEP_RESEARCH_LLM_MODEL` | `gpt-4o-mini` | Model name (e.g. `llama3`, `mistral`). Config: `deep_research.llm_model` |
| `DEEP_RESEARCH_SYNTHESIS` | `1` (enabled) | Set `0` to run search + scrape only (skip LLM step). Config: `deep_research.synthesis_enabled` |
| `DEEP_RESEARCH_SYNTHESIS_MAX_SOURCES` | `8` | Max source docs fed to LLM. Config: `deep_research.synthesis_max_sources` |
| `DEEP_RESEARCH_SYNTHESIS_MAX_CHARS_PER_SOURCE` | `2500` | Max chars per source. Config: `deep_research.synthesis_max_chars_per_source` |
| `DEEP_RESEARCH_SYNTHESIS_MAX_TOKENS` | `1024` | Max tokens in the LLM response. Tune per model: `512`–`1024` for small 4k-ctx models (e.g. `lfm2-2.6b`), `2048`+ for large models. Config: `deep_research.synthesis_max_tokens` |

---

## 🏆 Comparison

| Feature | Firecrawl / Jina / Tavily | Cortex Scout v3.1.0 |
|---------|--------------------------|-------------------|
| **Deep Research** | None / paid add-on | **Native: multi-hop + LLM synthesis (local or cloud)** |
| **Cost** | $49–$499/mo | **$0 — self-hosted** |
| **Privacy** | They see your queries | **100% private, local-only** |
| **Search Engine** | Proprietary / 3rd-party API | **Native Rust (4 engines, parallel)** |
| **Result Quality** | Mixed, noisy snippets | **Deduped, scored, LLM-clean** |
| **Cloudflare Bypass** | Rarely | **Native Chromium CDP + HITL fallback** |
| **LinkedIn / Airbnb** | Blocked | **99.99% success (HITL)** |
| **JS Rendering** | Cloud API | **Native Brave + bundled Chromium CDP** |
| **Semantic Memory** | None | **Embedded LanceDB + Model2Vec** |
| **Proxy Support** | Paid add-on | **Native SOCKS5/HTTP rotation** |
| **MCP Native** | Partial | **Full MCP stdio + HTTP** |


---

## 🤖 Agent Optimal Setup: IDE Copilot Instructions

Cortex Scout works best when your AI agent **knows the operational rules** before it starts — which tool to call first, when to rotate proxies, and when *not* to use `extract_structured`. Without these rules, agents waste tokens re-fetching cached data and can misuse tools on incompatible sources.

The complete rules file lives at [`.github/copilot-instructions.md`](.github/copilot-instructions.md) (VS Code / GitHub Copilot) and is also available as [`.clinerules`](.clinerules) for Cline. Copy the block below into the IDE-specific file for your editor.

---

### 🗂️ VS Code — `.github/copilot-instructions.md`

Create (or append to) `.github/copilot-instructions.md` in your workspace root:

```markdown
## MCP Usage Guidelines — Cortex Scout

### CortexScout Priority Rules

1. **Memory first (NEVER skip):** ALWAYS call `research_history` BEFORE calling `search_web`,
   `search_structured`, or `scrape_url`.
   **Cache-quality guard:** only skip a live fetch when ALL of the following are true:
   - similarity score ≥ 0.60
   - entry_type is NOT "search" (search entries have no word_count — always follow up with scrape_url)
   - word_count ≥ 50 (cached crates.io pages are JS-placeholders with ~11 words)
   - no placeholder/sparse warnings (placeholder_page, short_content, content_restricted)

2. **Initial research:** use `search_structured` (search + content summaries in one call).
   For private/internal tools not indexed publicly, skip search and go directly to
   `scrape_url` on the known repo/docs URL.

3. **Doc/article pages:** `scrape_url` with `output_format: clean_json`,
   `strict_relevance: true`, `query: "<your question>"`.
   Raw `.md`/`.txt` URLs are auto-detected — HTML pipeline is skipped, raw content returned.

4. **Proxy rotation (mandatory on first block):** if `scrape_url` or `search_web` returns
   403/429/rate-limit, immediately call `proxy_manager` with `action: "grab"` then retry
   with `use_proxy: true`. Do NOT wait for a second failure.

4a. **Auto-escalation on low confidence:** if `scrape_url` returns confidence < 0.3 or
    extraction_score < 0.4 → retry with `quality_mode: "aggressive"` → `visual_scout`
    → `human_auth_session`. Never stay stuck on a low-confidence result.

5. **Schema extraction:** use `fetch_then_extract` (one-shot) or `extract_structured`.
   Both auto-inject `raw_markdown_url` warning when called on raw file URLs.
   Do NOT point at raw `.md`/`.json`/`.txt` unless intentional.

6. **Sub-page discovery:** use `crawl_website` before `scrape_url` when you only know
   an index URL and need to find the right sub-page.

7. **Last resort:** `non_robot_search` only after direct fetch + proxy rotation have both
   failed (Cloudflare / CAPTCHA / login walls). Session cookies are persisted after login.
```

---

### 🐾 Cursor — `.cursorrules`

Create or append to `.cursorrules` in your project root with the same block above.

---

### 🟩 Cline (VS Code extension) — `.clinerules`

Already included in this repository as [`.clinerules`](.clinerules). Cline loads it automatically — no action needed.

---

### 🧠 Claude Desktop — System Prompt / Custom Instructions

Paste the rules block into the **Custom Instructions** or **System Prompt** field in Claude Desktop settings (Settings → Advanced → System Prompt).

---

### 🧳 Other Agents (Windsurf, Aider, Continue, AutoGen, etc.)

Any agent that accepts a system prompt or workspace instruction file: paste the same block. The rules are plain markdown and tool-agnostic.

---

### Quick Decision Flow

```
Question / research task
        │
        ▼
research_history ──► hit (≥ 0.60)? ──► cache-quality guard:
        │ miss            │  entry_type=="search"? ──► don't skip; do scrape_url
        │                 │  word_count < 50 or placeholder warnings? ──► don't skip
        │                 └──► quality OK? ──► use cached result, STOP
        │
        ▼
search_structured ──► enough content? ──► use it, STOP
        │ need deeper page
        ▼
scrape_url (clean_json + strict_relevance + query)
  │ confidence < 0.3 or extraction_score < 0.4?
  ├──► retry quality_mode: aggressive ──► visual_scout ──► human_auth_session
  │ 403/429/blocked? ──► proxy_manager grab ──► retry use_proxy: true
  │ still blocked? ──► non_robot_search  (LAST RESORT)
  │
  └── need schema JSON? ──► fetch_then_extract (schema + strict=true)
```

> 📖 Full rules + per-tool quick-reference table: [`.github/copilot-instructions.md`](.github/copilot-instructions.md)

---

## v3.0.0 (2026-02-20)

### Added

- **`human_auth_session` (The Nuclear Option)**: Launches a visible browser for human login/CAPTCHA solving. Captures and persists full authentication cookies to `~/.cortex-scout/sessions/{domain}.json`. Enables full automation for protected URLs after a single manual session.
- **Instruction Overlay**: `human_auth_session` now displays a custom green "Cortex Scout" instruction banner on top of the browser window to guide users through complex auth walls.
- **Persistent Session Auto-Injection**: `web_fetch`, `web_crawl`, and `visual_scout` now automatically check for and inject matching cookies from the local session store.
- **`extract_structured` / `fetch_then_extract`**: new optional params `placeholder_word_threshold` (int, default 10) and `placeholder_empty_ratio` (float 0–1, default 0.9) allow agents to tune placeholder detection sensitivity per-call.
- **`web_crawl`**: new optional `max_chars` param (default 10 000) caps total JSON output size to prevent workspace storage spill.
- **Rustdoc module extraction**: `extract_structured` / `fetch_then_extract` correctly populate `modules: [...]` on docs.rs pages using the `NAME/index.html` sub-directory convention.
- **GitHub Discussions & Issues hydration**: `fetch_via_cdp` detects `github.com/*/discussions/*` and `/issues/*` URLs; extends network-idle window to 2.5 s / 12 s max and polls for `.timeline-comment`, `.js-discussion`, `.comment-body` DOM nodes.
- **Contextual code blocks** (`clean_json` mode): `SniperCodeBlock` gains a `context: Option<String>` field. Performs two-pass extraction for prose preceding fenced blocks and Markdown sentences containing inline snippets.
- **IDE copilot-instructions guide** (README): new `🤖 Agent Optimal Setup` section.
- **`.clinerules`** workspace file: all 7 priority rules + decision-flow diagram + per-tool quick-reference table.
- **Agent priority rules in tool schemas**: every MCP tool description now carries machine-readable `⚠️ AGENT RULE` / `✅ BEST PRACTICE`.

### Changed

- **Placeholder detection (Scalar-Only Logic)**: Confidence override to 0.0 now only considers **scalar (non-array)** fields. Pure-array schemas (headers, modules, structs) never trigger fake placeholder warnings, fixing false-positives on rich but list-heavy documentation pages.
- `web_fetch(output_format="clean_json")`: applies a `max_chars`-based paragraph budget and emits `clean_json_truncated` when output is clipped.
- `extract_fields` / `fetch_then_extract`: placeholder/unrendered pages (very low content + mostly empty schema fields) force `confidence=0.0`.
- **Short-content bypass** (`strict_relevance` / `extract_relevant_sections`): early exit with a descriptive warning when `word_count < 200`. Short pages (GitHub Discussions, Q&A threads) are returned whole.

### Fixed

- **BUG-6**: `modules: []` always empty on rustdoc pages — refactored regex to support both absolute and simple relative module links (`init/index.html`, `optim/index.html`).
- **BUG-7**: false-positive `confidence=0.0` on real docs.rs pages; replaced whole-schema empty ratio with scalar-only ratio + raised threshold.
- **BUG-9**: `web_crawl` could spill 16 KB+ of JSON into VS Code workspace storage; handler now truncates response to `max_chars` (default 10 000).
- `web_fetch(output_format="clean_json")`: paragraph filter now adapts for `word_count < 200`.
- `fetch_then_extract`: prevents false-high confidence on JS-only placeholder pages (e.g. crates.io) by overriding confidence to 0.0.
- **`cdp_fallback_failed` on GitHub Discussions**: extended CDP hydration window and selector polling ensures full thread capture.

### ☕ Acknowledgments & Support

Cortex Scout is built with ❤️ by a **solo developer** for the open-source AI community.
If this tool saved you from a $500/mo scraping API bill:

- ⭐ **Star the repo** — it helps others discover this
- 🐛 **Found a bug?** [Open an issue](https://github.com/DevsHero/cortex-scout/issues)
- 💡 **Feature request?** Start a discussion
- ☕ **Fuel more updates:**

[![Sponsor](https://img.shields.io/static/v1?label=Sponsor&message=%E2%9D%A4&logo=GitHub&color=ff69b4&style=for-the-badge)](https://github.com/sponsors/DevsHero)

**License:** MIT — free for personal and commercial use.

