## MCP Tool Usage Rules (CortexAST + Shadowcrawl)

### CortexAST Priority Rules

**The Golden Rule (Non‑Negotiable):**
- NEVER use standard IDE/shell tools (`grep`, `rg`, `cat`, `head`, `tree`, `ls`, `git diff`) for codebase exploration, symbol lookup, or refactor verification.
- ALWAYS use CortexAST Megatools. They are AST-accurate, token-efficient, and designed to keep agents on rails.
- If a tool returns an error telling you which parameter you forgot, treat it as an instruction and retry the tool call (do not guess).

**Megatool Quick‑Reference**

| Task | Megatool | Action Enum | Required Params |
|---|---|---|---|
| Repo overview (files + public symbols) | `cortex_code_explorer` | `map_overview` | `target_dir` (use `.` for whole repo) |
| Token-budgeted context slice (XML) | `cortex_code_explorer` | `deep_slice` | `target` |
| Extract exact symbol source | `cortex_symbol_analyzer` | `read_source` | `path` + `symbol_name` *(or `path` + `symbol_names` for batch)* |
| Find all usages before signature change | `cortex_symbol_analyzer` | `find_usages` | `symbol_name` + `target_dir` |
| Find trait/interface implementors | `cortex_symbol_analyzer` | `find_implementations` | `symbol_name` + `target_dir` |
| Blast radius before rename/move/delete | `cortex_symbol_analyzer` | `blast_radius` | `symbol_name` + `target_dir` |
| Cross-boundary update checklist | `cortex_symbol_analyzer` | `propagation_checklist` | `symbol_name` *(or legacy `changed_path`)* |
| Save pre-change snapshot | `cortex_chronos` | `save_checkpoint` | `path` + `symbol_name` + `semantic_tag` |
| List snapshots | `cortex_chronos` | `list_checkpoints` | *(none)* |
| Compare snapshots (AST diff) | `cortex_chronos` | `compare_checkpoint` | `symbol_name` + `tag_a` + `tag_b` *(use `tag_b="__live__"` + `path` to diff against current state)* |
| Delete old snapshots (housekeeping) | `cortex_chronos` | `delete_checkpoint` | `symbol_name` and/or `semantic_tag` *(optional: `path`, `namespace`)* — OR `namespace` alone to purge all checkpoints in that namespace |
| Compile/lint diagnostics | `run_diagnostics` | *(none)* | `repoPath` |

## The Ultimate CortexAST Refactoring SOP

Whenever you are asked to perform a non-trivial refactor or update a core feature, you MUST generate and print this Markdown checklist into the chat **before writing any code**, and check the boxes as you proceed:

### 🧠 Refactoring Orchestration Plan
- [ ] **Phase 1: Recon & Blast Radius**
  - [ ] Use `map_overview` to understand the domain.
  - [ ] Use `blast_radius` (or `find_usages`) on the target symbol.
- [ ] **Phase 2: Snapshot**
  - [ ] Use `save_checkpoint` on the target files/symbols.
- [ ] **Phase 3: Execution**
  - [ ] Read minimal context using `read_source` (`skeleton_only: true` if large).
  - [ ] Write the code edits.
- [ ] **Phase 4: Verification & Sync**
  - [ ] Use `run_diagnostics` to catch compiler errors.
  - [ ] Use `compare_checkpoint` to verify structural intent.
  - [ ] Use `propagation_checklist` to ensure TS/Python/Proto boundaries are updated.

**The Autonomous Refactoring Flow (Rails)**

Follow this sequence for any non-trivial refactor (especially renames, signature changes, or cross-module work):

1. **Explore** → `cortex_code_explorer(action: map_overview)`
2. **Isolate** → `cortex_symbol_analyzer(action: read_source)` (get the exact symbol source before editing)
3. **Measure Impact** →
  - Use `cortex_symbol_analyzer(action: find_usages)` BEFORE changing any signature
  - Use `cortex_symbol_analyzer(action: blast_radius)` BEFORE any rename/move/delete
4. **Checkpoint** → `cortex_chronos(action: save_checkpoint, semantic_tag: pre-refactor)`
5. **Edit Code** → make the minimal change
6. **Verify** →
  - `run_diagnostics` immediately after editing
  - `cortex_chronos(action: compare_checkpoint)` to verify semantics (never use `git diff`); prefer `tag_b="__live__"` for "before vs now"
7. **Cross‑Sync** → `cortex_symbol_analyzer(action: propagation_checklist)` when touching shared types/contracts

**Output safety (spill prevention):**
- Output is truncated server-side at `max_chars` (default **8000**). VS Code Copilot writes responses larger than ~8 KB to workspace storage — the 8000 default is calibrated to stay below that threshold. Set `max_chars` explicitly (e.g. `3000`) for large-scope queries; increase only if your client handles larger inline output.

**`repoPath` best practice:**
- Always pass `repoPath` explicitly on every tool call (e.g. `repoPath="/Users/me/project"`). Without it, the server tries `git rev-parse --show-toplevel` → `initialize` workspace root → `cwd`, but VS Code may spawn the MCP server with `$HOME` as cwd, causing all path resolution to fail silently.
- Use the absolute workspace root path, not a subdirectory.

**Propagation best practice (Hybrid Omni‑Match):**
- `propagation_checklist` automatically matches common casing variants of `symbol_name` (PascalCase / camelCase / snake_case).
- When a symbol is renamed across boundaries (e.g. Rust `TrainingEngineCapabilities` → TS `trainingCaps`), pass `aliases: ["trainingCaps"]` to catch cross-language usage without heavy import tracing.

# MCP Usage Guidelines — ShadowCrawl

These rules encode the optimal agent workflow for ShadowCrawl tools.
They are hardcoded here so every AI agent working in this repository follows
the same efficient, hallucination-resistant research process.

---

## Shadowcrawl Priority Rules

### 1. Memory Before Search (mandatory)
- ALWAYS call `memory_search` BEFORE calling `web_search` or `web_search_json`
- If a result is returned with similarity score ≥ 0.60, use the cached data directly
  and skip the live fetch entirely
- Only proceed to a fresh live search when memory returns no relevant hit

### 2. Prefer `web_search_json` Over `web_search` + `web_fetch`
- `web_search_json` combines search + pre-scraped content summaries in a **single call**
- Use `web_search_json` as the **default first step** for any research task
- Only fall back to `web_search` (without content) when you specifically need raw URLs only

### 3. Use `web_fetch` with Noise Reduction for Documentation Pages
- For documentation, article, or tutorial pages always set:
  ```
  output_format: "clean_json"
  strict_relevance: true
  query: "<your specific question>"
  ```
- This strips 100 % of nav/footer/boilerplate and keeps only query-relevant paragraphs
- Token savings are typically 60–80 % compared to raw text output
- If you see `clean_json_truncated` in warnings, increase `max_chars` (the tool clips large pages to prevent output spilling).
- Note: semantic shaving intentionally bypasses when `word_count < 200` (short pages are returned whole).

### 4. Rotate Proxy on First Block Signal (mandatory)
- If `web_fetch` or `web_search` returns **403 / 429 / rate-limit / IP-block**:
  1. Immediately call `proxy_control` with `action: "grab"`
  2. Retry the failed call with `use_proxy: true`
- Do NOT retry the same call without rotating first; do NOT escalate to `hitl_web_fetch`
  until proxy rotation has also failed

### 5. Structured Extraction — `fetch_then_extract` / `extract_fields`

Use schema-driven extraction when you need a stable JSON shape for downstream agent logic.

- Prefer `fetch_then_extract` for **one-shot** workflows (fetch + extract in the same tool call).
- Use `extract_fields` when you already fetched/scraped content (or when the agent framework routes through it).

Strict mode (default):
- `strict: true` enforces schema shape (no drift):
  - missing array-like fields → `[]`
  - missing scalar fields → `null`
- In strict mode with an explicit schema, metadata keys like `_title` / `_url` are suppressed.

Confidence safety (mandatory):
- `confidence == 0.0` + `placeholder_page` warning fires **only when BOTH of the following are true**:
  1. `word_count < placeholder_word_threshold` (default 10) **or** content has ≤ 1 non-empty line
  2. ≥ `placeholder_empty_ratio` (default 0.9) of **non-array** schema fields are null/empty string
  - **Empty arrays are excluded from condition 2** — `[]` is a valid "no items found" result and is
    never counted as a placeholder signal. A schema with only array fields (`structs`, `modules`, etc.)
    will **never** trigger confidence=0.0 via this path.
  - When fired: the page is an unrendered JS-only shell (e.g. crates.io, npm). Do NOT trust any fields.
    Escalate to CDP/browser rendering or HITL auth tools.
  - When NOT fired: partial extraction is valid — e.g. `structs: [32 items]` + `modules: []` stays
    above 0.0 because `structs` is a non-empty array (no scalar fields to fail the check).
- Tuning: pass `placeholder_word_threshold` (integer) and `placeholder_empty_ratio` (0–1 float) to
  `extract_structured` / `fetch_then_extract` when the defaults cause false positives or false negatives.

Input constraint:
- Don’t point structured extraction at raw `.md` / `.json` / `.txt` unless that’s intentionally your source.
  For docs pages, prefer `web_fetch(output_format="clean_json")` then extract.

### 6. `web_crawl` — Use When Sub-Page Discovery Is Needed
- Use `web_crawl` when you know a doc site's index URL but do not know which sub-page
  holds the information you need
- Do NOT assume a single `web_fetch` of the index page is sufficient for large doc sites
- Typical workflow: `web_crawl` to discover links → `web_fetch` on the specific sub-page
- Output is capped at `max_chars` (default 10 000) to prevent workspace storage spill.
  Pass a higher value (e.g. `max_chars: 30000`) when crawling many pages.

### 7. `hitl_web_fetch` — Last Resort Only
- Use ONLY when both direct fetch AND proxy rotation have failed
- Intended for: heavy Cloudflare challenges, CAPTCHA, login walls
- Do NOT use as a first attempt for any site — always try automated methods first

---

## Decision Flow Summary

```
Question / research task
        │
        ▼
memory_search ──► hit (≥ 0.60)? ──► use cached result, STOP
        │ miss
        ▼
web_search_json ──► enough content? ──► use it, STOP
        │ need deeper page
        ▼
web_fetch (clean_json + strict_relevance + query)
  │ need schema-stable JSON?
  ▼
fetch_then_extract (schema + strict=true)
        │ 403/429/blocked?
        ▼
proxy_control grab ──► retry web_fetch with use_proxy: true
        │ still blocked?
        ▼
hitl_web_fetch  (LAST RESORT)
```

---

## Tool Quick-Reference

| Tool | When to use | When NOT to use |
|---|---|---|
| `memory_search` | First step, before every search | — |
| `web_search_json` | Initial research (search + content) | When only raw URLs needed |
| `web_search` | Raw URL list only | As substitute for `web_search_json` |
| `web_fetch` | Fetching a specific known URL | As primary research step |
| `web_fetch` `clean_json` | Documentation / article pages (token-efficient) | When you need full raw HTML (use `json` + `include_raw_html=true`, but expect large output) |
| `web_crawl` | Doc site sub-page discovery | Single-page fetches |
| `fetch_then_extract` | One-shot fetch + strict schema extraction | When you only need prose; use `web_fetch(clean_json)` instead |
| `extract_fields` | Strict schema extraction from scraped pages | Raw .md / .json / .txt files (unless intentional) |
| `proxy_control` | After any 403/429 error | Proactively without a block signal |
| `hitl_web_fetch` | CAPTCHA / login wall | Any automatable page |
