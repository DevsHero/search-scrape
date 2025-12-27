# Response to AI Reviewer Feedback

## Summary: The reviewer is **CORRECT** ✅

Thank you for the detailed analysis! You've identified real limitations in our current implementation.

---

## What We're Already Doing Right

✅ **We extract structured metadata** - We have `canonical_url`, `published_at`, `author`, `og_image`, etc. internally  
✅ **We preserve raw HTML** - The `content` field stores original HTML  
✅ **Smart content extraction** - Multiple fallback strategies (readability → heuristic → full text)  
✅ **Smart link filtering** - `extract_content_links()` prioritizes article/main content  

### The Problem

We're **throwing away** this structured data by converting it to formatted text for MCP output!

Current flow:
```rust
ScrapeResponse (JSON-ready structs) 
    → format!() to markdown text 
    → Return plain string to MCP tool
```

---

## Issues Confirmed & Assessment

| Issue | Valid? | Status | Difficulty |
|-------|--------|--------|-----------|
| Plain text output instead of JSON | ✅ YES | Can fix now | EASY |
| Code blocks corrupted (whitespace lost) | ✅ YES | Can fix now | MEDIUM |
| Metadata only in text, not machine fields | ✅ YES | Can fix now | EASY |
| No JS-render fallback | ✅ YES | Future work | HARD |
| Search noise (Rust game in "rust async") | ✅ YES | Can improve | MEDIUM |

---

## Priority 1 Fixes (Implement Today)

### 1. ✅ Add JSON Output Format

**Change:** Add `output_format` parameter to both MCP tools
- `text` (default) - Current markdown-style formatting for human readability
- `json` - Return raw ScrapeResponse/SearchResponse as JSON string

**Why not always JSON?** Some users/tools prefer formatted text in chat interfaces.

### 2. ✅ Fix Code Block Preservation

**Problem:** `html2text::from_read()` collapses whitespace/newlines in `<pre><code>` blocks

**Solution:** 
- Extract code blocks **before** html2text conversion
- Preserve them separately with language hints
- Add `code_blocks: Vec<CodeBlock>` to response
- When formatting text, insert code blocks with proper fencing

```rust
pub struct CodeBlock {
    pub language: Option<String>,  // from class="language-rust"
    pub code: String,               // raw code with \n preserved
    pub start_char: Option<usize>,
    pub end_char: Option<usize>,
}
```

### 3. ✅ Add Machine-Readable Flags

Add to `ScrapeResponse`:
```rust
pub truncated: bool,
pub actual_chars: usize,
pub max_chars_limit: Option<usize>,
pub extraction_score: Option<f64>,  // 0.0-1.0 quality heuristic
pub warnings: Vec<String>,
```

**Extraction Score Logic:**
```rust
score = 
    (word_count > 50 ? 0.3 : 0.0) +           // Has content
    (published_at.is_some() ? 0.2 : 0.0) +    // Has date
    (code_blocks.len() > 0 ? 0.2 : 0.0) +     // Has code
    (word_count / expected_length * 0.3)       // Content ratio
```

### 4. ✅ Expose Raw HTML Option

Add `include_html: bool` parameter - when true, include full `html_content` field in JSON response.

---

## Priority 2 Fixes (Next Week)

### 5. ⏳ Playwright JS Fallback

**When:** If `extraction_score < 0.4` or `word_count < 50` or `js_required` detected

**How:** 
- Add optional Playwright dependency
- Env var: `PLAYWRIGHT_FALLBACK=true`
- Re-fetch and re-extract with rendered DOM

**Estimated work:** 2-3 days (new dependency, process management, error handling)

### 6. ⏳ Search Result Filtering

**Problem:** "rust async" returns Rust game, Steam, etc.

**Solution:** 
- Add `source_type` heuristic: `docs|repo|blog|news|other`
- Domain whitelist for dev queries: `*.github.io`, `docs.rs`, `rust-lang.org`, etc.
- Add `prefer_docs: bool` parameter
- Boost ranking for matches

### 7. ⏳ Structured Metadata Extraction

Already extracting:
- ✅ `canonical_url`
- ✅ `published_at` (article:published_time)
- ✅ `author`
- ✅ `og_title`, `og_description`, `og_image`

**Need to add:**
- JSON-LD parsing (`<script type="application/ld+json">`)
- Twitter Card meta tags
- Article schema data

---

## Implementation Plan

### Phase 1 (Today): JSON Output + Flags
1. Add `output_format` param to MCP schema ✅
2. Add structured fields to response ✅
3. Update stdio_service.rs to conditionally format ✅
4. Update mcp.rs HTTP endpoints ✅

### Phase 2 (Tomorrow): Code Block Preservation
1. Add `CodeBlock` struct ✅
2. Extract `<pre><code>` before html2text ✅
3. Preserve language hints ✅
4. Re-inject with fencing in text mode ✅

### Phase 3 (Next Week): Quality Improvements
1. Add extraction_score calculation ✅
2. Playwright integration (optional) ⏳
3. Search filtering improvements ⏳

---

## Proposed JSON Schema (Matches Your Spec)

```json
{
  "url": "https://thenewstack.io/async-programming-in-rust-understanding-futures-and-tokio/",
  "canonical_url": "https://thenewstack.io/async-programming-in-rust-understanding-futures-and-tokio/",
  "title": "Async Programming in Rust: Understanding Futures and Tokio - The New Stack",
  "domain": "thenewstack.io",
  "lang": "en-US",
  "publish_date": null,
  "author": null,
  "fetch_time": "2025-12-28T10:15:00Z",
  "max_chars_limit": 15000,
  "actual_chars": 15816,
  "truncated": true,
  "extraction_score": 0.84,
  "warnings": ["content_truncated"],
  "meta_description": "Combined with powerful runtime libraries like Tokio...",
  "headings": [
    {"level": 1, "text": "Async Programming in Rust: Understanding Futures and Tokio"}
  ],
  "code_blocks": [
    {
      "language": "rust",
      "code": "use std::future::Future;\nuse std::pin::Pin;\n...",
      "start_char": 456,
      "end_char": 678
    }
  ],
  "text_content": "(cleaned body text with preserved code formatting)",
  "html_content": "(optional, only if include_html=true)",
  "links_found": 244,
  "images_found": 4,
  "links": [
    {"url": "https://...", "text": "link text", "rel": "nofollow"}
  ],
  "images": [
    {"url": "https://...", "alt": "image description", "width": 800, "height": 600}
  ],
  "word_count": 1274,
  "reading_time_minutes": 7,
  "status_code": 200,
  "content_type": "text/html; charset=utf-8"
}
```

---

## Response to Specific Points

### "output is plain human text (hard to parse reliably by agents)"
**Agreed.** We'll add `output_format=json` option. Default stays `text` for backward compatibility.

### "code blocks are corrupted or whitespace-newlines lost"
**Agreed.** This is a real bug. We'll extract code blocks separately before html2text conversion.

### "key metadata only in text — not machine fields"
**Agreed.** We have the data internally but format it as text. Will expose in JSON mode.

### "no JS-render fallback when extraction fails"
**Agreed.** This is a major limitation for SPAs. Will add Playwright as optional fallback with env flag.

### "search results contain noisy unrelated results"
**Agreed.** We can improve this with domain filtering and source-type classification.

---

## Timeline

- **Today:** JSON output format + structured flags
- **Tomorrow:** Code block preservation fix
- **Next week:** Extraction score + optional Playwright
- **Future:** Advanced search filtering

---

## Questions for You

1. Should JSON be the **default** format, with `output_format=text` as opt-in? (Breaking change)
2. Should we always extract code blocks, or only when `output_format=json`?
3. For Playwright fallback, should it be automatic or require explicit `js_render=true` param?
4. Should we version the API (`/v1/`, `/v2/`) to handle breaking changes?

---

## Conclusion

Your analysis is **spot-on**. The core extraction logic is solid, but we're losing value by converting everything to text. The fixes are straightforward and high-impact.

**Estimated implementation time:**
- Priority 1 fixes: 4-6 hours
- Priority 2 fixes: 2-3 days
- Total for all improvements: ~1 week

Thank you for the detailed review! This will make the tools significantly better for agent use cases.
