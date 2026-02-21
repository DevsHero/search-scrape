## MCP Tool Usage Rules (CortexAST + Shadowcrawl)

### CortexAST Priority Rules

**The Golden Rule (Non‑Negotiable):**
- NEVER use standard IDE/shell tools (`grep`, `rg`, `cat`, `head`, `tree`, `ls`, `git diff`) for codebase exploration, symbol lookup, or refactor verification.
- ALWAYS use CortexAST Megatools. They are AST-accurate, token-efficient, and designed to keep agents on rails.
- If a tool returns an error telling you which parameter you forgot, treat it as an instruction and retry the tool call (do not guess).

**Megatool Quick‑Reference**

| Task | Megatool | Action Enum | Required Params | Key Optional Params |
|---|---|---|---|---|
| Repo overview (files + public symbols) | `cortex_code_explorer` | `map_overview` | `target_dir` (use `.` for whole repo) | `exclude` (dir-name array), `search_filter`, `max_chars`, `ignore_gitignore` |
| Token-budgeted context slice (XML) | `cortex_code_explorer` | `deep_slice` | `target` | `exclude` (dir-name array), `budget_tokens`, `skeleton_only`, `query`, `query_limit` |
| Extract exact symbol source | `cortex_symbol_analyzer` | `read_source` | `path` + `symbol_name` *(or `path` + `symbol_names` for batch)* | `instance_index` (0-based), `skeleton_only` |
| Find all usages before signature change | `cortex_symbol_analyzer` | `find_usages` | `symbol_name` + `target_dir` | |
| Find trait/interface implementors | `cortex_symbol_analyzer` | `find_implementations` | `symbol_name` + `target_dir` | |
| Blast radius before rename/move/delete | `cortex_symbol_analyzer` | `blast_radius` | `symbol_name` + `target_dir` | |
| Cross-boundary update checklist | `cortex_symbol_analyzer` | `propagation_checklist` | `symbol_name` *(or legacy `changed_path`)* | `aliases` |
| Save pre-change snapshot | `cortex_chronos` | `save_checkpoint` | `path` + `symbol_name` + `semantic_tag` | `namespace` |
| List snapshots | `cortex_chronos` | `list_checkpoints` | *(none)* | `namespace` |
| Compare snapshots (AST diff) | `cortex_chronos` | `compare_checkpoint` | `symbol_name` + `tag_a` + `tag_b` *(use `tag_b="__live__"` + `path` to diff against current state)* | `namespace`, `path` |
| Delete old snapshots (housekeeping) | `cortex_chronos` | `delete_checkpoint` | `symbol_name` and/or `semantic_tag` *(optional: `path`, `namespace`)* — Automatically searches legacy flat `checkpoints/` if no matches in namespace. | `namespace` |
| Compile/lint diagnostics | `run_diagnostics` | *(none)* | `repoPath` | |

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

**`exclude` best practice (map_overview + deep_slice):**
- If `map_overview` returns “Massive Directory” or counts look inflated (e.g. `node_modules/`, build outputs, generated code), pass `exclude: ["node_modules", "vendor", "__pycache__", "build", "dist"]`.
- `exclude` matches directory **base names** (not full paths) and prunes at every depth.
- Prefer `exclude` over widening `target_dir` — it keeps scans focused and avoids summary-only mode.

**`instance_index` best practice (read_source):**
- Some files legitimately contain multiple same-named definitions. When that happens, `read_source` will prepend a “Disambiguation: Found N instances…” header.
- Default is instance 0. To select a different one, pass `instance_index: 1` (second), `2` (third), etc.
- If your goal is “where is this used?”, use `find_usages` first; if your goal is “show me all defs”, call `read_source` repeatedly with different `instance_index` values.

**`deep_slice` path hint behavior:**
- If you pass a target that doesn’t exist (e.g. `"orchestrator"` instead of `"orchestrator.rs"`), the server may return “Did you mean one of: …” suggestions. Use one of the suggested paths to avoid a retry loop.

**`compare_checkpoint` identical behavior:**
- If the two snapshots are identical, Chronos will return a compact “✅ NO STRUCTURAL DIFF” summary instead of printing the full symbol twice.
**`delete_checkpoint` source label:**
- When deletions come from a named namespace directory, the confirmation reads: `"Deleted N/M checkpoint(s) from namespace 'name' (path)"`.
- When no matches exist in the namespace dir and the legacy flat `checkpoints/` directory is searched instead, the label switches to `"legacy flat store (path)"`. This distinction tells you whether the deleted checkpoints were created under the old pre-namespace layout.
**`repoPath` best practice:**
- Always pass `repoPath` explicitly on every tool call (e.g. `repoPath="/Users/me/project"`) when you know the path. Without it, the server uses the root established by the MCP `initialize` handshake.
- **CRITICAL Safeguard:** For safety, if CortexAST resolves to a "dead root" (OS root or Home directory), it returns a **CRITICAL error** instead of proceeding. You MUST catch this and provide the correct `repoPath`.
- Use the absolute workspace root path, not a subdirectory.
- **Server owners**: configure `--root /absolute/path/to/project` in your MCP server args (or set `CORTEXAST_ROOT` env var). This is the most reliable fix — it sets the default root at server startup so every tool call resolves correctly even without an explicit `repoPath`. Example VS Code `settings.json`: `"args": ["mcp", "--root", "/Users/me/my-project"]`.
- **Root resolution order** (first non-dead value wins):
  1. `repoPath` param in the tool call — per-call override
  2. Root from the MCP `initialize` request (`rootUri` / `rootPath` / `workspaceFolders[0].uri`) — **the authoritative protocol-level value**; overwrites the bootstrap
  3. `--root` CLI flag / `CORTEXAST_ROOT` env var — startup bootstrap
  4. IDE env vars: `VSCODE_WORKSPACE_FOLDER`, `VSCODE_CWD`, `IDEA_INITIAL_DIRECTORY`, `PWD`/`INIT_CWD` (if ≠ `$HOME`) — checked both at startup AND directly inside every tool call (belt-and-suspenders)
  5. Find-up heuristic on the tool's own `path` / `target_dir` / `target` argument — walks ancestors looking for `.git`, `Cargo.toml`, `package.json`
  6. `cwd` — **refused if it equals `$HOME` or OS root** (CRITICAL error)

**Propagation best practice (Hybrid Omni‑Match):**
- `propagation_checklist` automatically matches common casing variants of `symbol_name` (PascalCase / camelCase / snake_case).
- When a symbol is renamed across boundaries (e.g. Rust `TrainingEngineCapabilities` → TS `trainingCaps`), pass `aliases: ["trainingCaps"]` to catch cross-language usage without heavy import tracing.

# MCP Usage Guidelines — ShadowCrawl

These rules encode the optimal agent workflow for ShadowCrawl tools.
They are hardcoded here so every AI agent working in this repository follows
the same efficient, hallucination-resistant research process.

---

## Shadowcrawl Priority Rules

### 1. Memory Before Search (mandatory — NEVER skip)
- **ALWAYS** call `research_history` BEFORE calling `search_web`, `search_structured`, **or** `scrape_url`
- If a result is returned with similarity score ≥ 0.60, use the cached data directly
  and skip the live fetch entirely **ONLY IF** it passes the cache-quality guard below
- Only proceed to a fresh live search/fetch when `research_history` returns no relevant hit
- This rule applies to EVERY research cycle, including retries and follow-up fetches

**Cache-quality guard (mandatory):**
- If the cached entry indicates `word_count < 50` (or similarly sparse content), treat it as a **low-quality scrape** and DO NOT skip a live fetch.
- If the cached entry contains warnings that imply placeholder/blocked/sparse content (e.g. `placeholder_page`, `short_content`, `content_restricted`, `low_extraction_score`), treat it as **low-quality** and DO NOT skip a live fetch.
- If the cached entry has `entry_type == "search"` (a search-index summary, not a page scrape), **do NOT treat a similarity ≥ 0.60 hit as a reason to skip `scrape_url`**. Search entries carry no per-page `word_count` metadata — always follow up with a fresh `scrape_url` on the top result URL.
- Canonical example: `https://crates.io/crates/{name}` is often JS-rendered and can return a tiny placeholder. For Rust crates, prefer a fresh fetch on `https://docs.rs/crate/{name}/latest` (server-rendered) when cache looks sparse.

### 1a. Dynamic Parameters — Always Tune for the Task (mandatory)
- **`max_chars` controls the TOTAL serialized output payload**, not just the text field.
  Increase it for large pages (e.g. `max_chars: 50000`), decrease for tight token budgets.
- **Agent-tunable parameters** — set these to match your task:
  | Param | Default | When to change |
  |---|---|---|
  | `max_chars` | 10000 | Increase for deep content; decrease for summaries |
  | `snippet_chars` | 120/200 | Increase for detailed research snippets |
  | `max_headings` | 10 | Decrease for summary output |
  | `max_images` | 3 | Increase for image-rich pages |
  | `short_content_threshold` | 50 | Adjust for minimal/sparse pages |
  | `extraction_score_threshold` | 0.4 | Lower for low-quality HTML pages |
  | `placeholder_word_threshold` | 10 | Tune for JS-heavy pages |

### 2. Prefer `web_search_json` Over `web_search` + `web_fetch`
- `search_structured` combines search + pre-scraped content summaries in a **single call**
- Use `search_structured` as the **default first step** for any research task
- Only fall back to `search_web` (without content) when you specifically need raw URLs only
- For private/internal/unlisted tools that won’t be indexed publicly: skip search and go directly to `scrape_url` on the known repo/docs URL (if you have it).

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
- **Raw file auto-detection**: when the URL ends in `.md`, `.mdx`, `.rst`, `.txt`, `.csv`, `.toml`, `.yaml`, or `.yml`, `clean_json` mode **automatically skips the HTML extraction pipeline** and returns the raw content directly — no duplicate frontmatter, no noise. The response will contain a `raw_markdown_url` warning. To read a raw GitHub file, prefer `scrape_url(output_format: "text")` for prose or `scrape_url(output_format: "clean_json")` for structured paragraphs.

### 4. Rotate Proxy on First Block Signal (mandatory)
- If `scrape_url` or `search_web` returns **403 / 429 / rate-limit / IP-block**:
  1. Immediately call `proxy_manager` with `action: "grab"`
  2. Retry the failed call with `use_proxy: true`
- Do NOT retry the same call without rotating first; do NOT escalate to `non_robot_search`
  until proxy rotation has also failed

### 4a. Auto-Escalation on Low Confidence (mandatory — no repeat prompt)
- If `scrape_url` returns `confidence < 0.3` OR `extraction_score < 0.4` in the response:
  1. **First**: retry with `quality_mode: "aggressive"` (triggers full CDP browser rendering)
  2. **If still failing**: automatically escalate to `visual_scout` to screenshot the page
  3. **If auth-wall confirmed**: escalate to `human_auth_session` WITHOUT waiting for further
     user instructions — surface the reason and proceed autonomously
- For `extract_structured` / `fetch_then_extract`: if response includes `confidence == 0.0`
  AND `placeholder_page` warning, immediately retry via `non_robot_search` (HITL/CDP)
- **Never** leave the agent stuck on a low-confidence result without attempting escalation

### 5. Structured Extraction — `fetch_then_extract` / `extract_structured`

Use schema-driven extraction when you need a stable JSON shape for downstream agent logic.

- Prefer `fetch_then_extract` for **one-shot** workflows (fetch + extract in the same tool call).
- Use `extract_structured` when you need schema extraction on an already-known URL (agent framework routes through it).
- **`raw_markdown_url` auto-warn**: both `extract_structured` and `fetch_then_extract` automatically inject a warning into `warnings[]` when the URL is a raw `.md`/`.mdx`/`.rst`/`.txt` file — fields will likely return `null` and confidence will be low. Use `web_fetch(output_format: "clean_json")` instead for raw Markdown sources.

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
  For docs pages, prefer `web_fetch(output_format="clean_json")` then extract.- If you receive a `raw_markdown_url` warning in the response, switch to `web_fetch` instead.
### 6. `web_crawl` — Use When Sub-Page Discovery Is Needed
- Use `web_crawl` when you know a doc site's index URL but do not know which sub-page
  holds the information you need
- Do NOT assume a single `web_fetch` of the index page is sufficient for large doc sites
- Typical workflow: `web_crawl` to discover links → `web_fetch` on the specific sub-page
- Output is capped at `max_chars` (default 10 000) to prevent workspace storage spill.
  Pass a higher value (e.g. `max_chars: 30000`) when crawling many pages.

### 7. `non_robot_search` — Last Resort Only
- Use ONLY when both direct fetch AND proxy rotation have failed
- Intended for: heavy Cloudflare challenges, CAPTCHA, login walls
- Do NOT use as a first attempt for any site — always try automated methods first
- After a successful HITL session, cookies are saved to `~/.shadowcrawl/sessions/{domain}.json` — future `web_fetch` calls to that domain are automatically authenticated

---

## Decision Flow Summary

```
Question / research task
        │
        ▼
research_history ──► hit (≥ 0.60)? ──► cache-quality guard:
        │ miss            │  entry_type=="search"? ──► do NOT skip scrape_url
        │                 │  word_count < 50 or placeholder warnings? ──► do NOT skip
        │                 └──► quality OK? ──► use cached result, STOP
        │
        ▼
search_structured ──► enough content? ──► use it, STOP
        │ need deeper page
        ▼
scrape_url (clean_json + strict_relevance + query)
  │ raw .md/.mdx/.rst/.txt URL? ──► HTML pipeline skipped, raw content returned + raw_markdown_url warning
  │
  │ confidence < 0.3 or extraction_score < 0.4?
  ├──► retry with quality_mode: aggressive (CDP rendering)
  │        │ still low? ──► visual_scout ──► human_auth_session (saves session cookies)
  │
  │ need schema-stable JSON?
  ▼
fetch_then_extract (schema + strict=true)
  │ raw_markdown_url warning? ──► switch to scrape_url(clean_json) instead
  │ confidence == 0.0 + placeholder_page? ──► non_robot_search (no repeat prompt needed)
  │
  │ 403/429/blocked?
  ▼
proxy_manager grab ──► retry scrape_url with use_proxy: true
        │ still blocked?
        ▼
non_robot_search  (LAST RESORT — persists session after login)
```

---

## Tool Quick-Reference

| Tool (MCP name) | When to use | When NOT to use |
|---|---|---|
| `research_history` | **First step**, before every search/fetch | — |
| `search_structured` | Initial research (search + content summaries, single call) | When only raw URLs needed |
| `search_web` | Raw URL list only | As substitute for `search_structured` |
| `scrape_url` | Fetching a specific known URL | As primary research step (use `search_structured` instead) |
| `scrape_url` `clean_json` | Documentation / article pages (token-efficient, noise-free) | Raw `.md`/`.txt` files — use `text` or `clean_json` (auto-detected) |
| `scrape_url` `json` | When you need the full structured ScrapeResponse | Large pages without `max_chars` — output will be capped at `max_chars` |
| `crawl_website` | Doc site sub-page link discovery | Single-page fetches |
| `fetch_then_extract` | One-shot fetch + strict schema extraction | When you only need prose; use `scrape_url(clean_json)` instead |
| `extract_structured` | Schema extraction; auto-warns on raw markdown URLs | Raw `.md`/`.json`/`.txt` files — check for `raw_markdown_url` warning |
| `proxy_manager` | After any 403/429/rate-limit error | Proactively without a block signal |
| `visual_scout` | Screenshot a page to check for auth-walls (`auth_risk_score ≥ 0.4`) | General content fetching |
| `human_auth_session` | Full HITL login — persists session cookies for future calls | Before trying `non_robot_search` first |
| `non_robot_search` | CAPTCHA / Cloudflare / login wall (last resort) | Any page automatable via proxy rotation |
