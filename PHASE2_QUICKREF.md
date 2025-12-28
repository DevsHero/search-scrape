# Quick Reference: Phase 2 Features

## Smart Query Rewriting

### Automatic Enhancements

Your queries are automatically enhanced when they match these patterns:

| You Type | Auto-Rewrites To | Why |
|----------|------------------|-----|
| `rust docs` | `rust docs site:doc.rust-lang.org` | Official Rust documentation |
| `python tutorial` | Suggestions shown (not auto-rewritten) | Multiple good options |
| `tokio error timeout` | `tokio error timeout site:stackoverflow.com` | Best for error Q&A |
| `crate reqwest` | `crate reqwest site:docs.rs` | Rust crate documentation |
| `how to use react` | `how to use react site:react.dev` | Official React docs |
| `django setup` | Suggestions shown | Multiple approaches |

### Supported Keywords (40+)

**Languages**: rust, python, javascript, typescript, go, java, c++, ruby, php, swift, kotlin, scala, haskell, elixir, clojure

**Frameworks**: react, vue, angular, svelte, next, django, flask, fastapi, express, tokio, actix, axum, rocket, spring, laravel, rails

**Concepts**: async, await, api, rest, graphql, docker, kubernetes, git, test, debug, deploy, error, bug

## Duplicate Detection

### How It Works

- Checks last 6 hours of search history
- Uses 0.9+ similarity threshold
- Semantic matching (understands related terms)
- Shows non-blocking warning

### Example

```
⚠️ Similar search found from 2 hours ago (similarity: 0.92).
Consider checking history first.
```

**What to do**: Use `research_history` tool to check what you found before.

## Enhanced Search Results

### When Query is Rewritten

```
🔍 Query Enhanced: 'rust docs' → 'rust docs site:doc.rust-lang.org'

Found 12 results for 'rust docs':
1. The Rust Programming Language
   URL: https://doc.rust-lang.org/book/
   ...
```

### When Suggestions Available

```
💡 Query Optimization Tips:
   1. python tutorial site:docs.python.org
   2. python tutorial documentation

Found 24 results for 'python tutorial':
...
```

## SearXNG Engine Priorities

Engines are now weighted for better developer results:

| Rank | Engine | Weight | Best For |
|------|--------|--------|----------|
| 1 | GitHub | 1.5x | Code, repos, issues |
| 2 | Stack Overflow | 1.4x | Q&A, troubleshooting |
| 3 | Google | 1.3x | Documentation, guides |
| 4 | DuckDuckGo | 1.2x | Privacy + dev content |
| 5 | Bing | 1.1x | General search |
| 6 | Brave | 1.0x | Alternative results |

## Testing Your Queries

### Good Test Queries

Try these to see Phase 2 in action:

```bash
# Should auto-rewrite:
"rust docs"           → site:doc.rust-lang.org
"tokio error"         → site:stackoverflow.com  
"crate serde"         → site:docs.rs

# Should show suggestions:
"python tutorial"     → Multiple doc sites suggested
"react hooks"         → react.dev suggested

# Should trigger duplicate warning (if you search twice):
"rust async"          → Warning on 2nd search within 6h
```

### Non-Dev Queries (Unchanged)

These queries are NOT modified (as expected):

```bash
"coffee shops near me"  → No changes
"weather forecast"      → No changes
"news today"            → No changes
```

## Configuration

### Restart SearXNG (After Setup)

```bash
docker-compose restart searxng
```

Required after initial setup to load new engine weights.

### Enable Full Features

```bash
# With history (enables duplicate detection):
SEARXNG_URL=http://localhost:8888 \
QDRANT_URL=http://localhost:6333 \
./target/release/search-scrape-mcp

# Without history (query rewriting only):
SEARXNG_URL=http://localhost:8888 \
./target/release/search-scrape-mcp
```

## Performance

| Feature | Overhead | Impact |
|---------|----------|--------|
| Query rewriting | <1ms | ✅ Negligible |
| Duplicate check | 10-20ms | ✅ Negligible |
| Total | <25ms | ✅ <5% of search time |

## Troubleshooting

### "Query wasn't rewritten"

**Likely reasons**:
1. Query is not developer-related
2. Query already has `site:` filter
3. No clear pattern match

**What to do**: Check suggestions section for alternatives.

### "No duplicate warning but I searched before"

**Possible causes**:
1. More than 6 hours ago
2. Similarity < 0.9 (too different)
3. QDRANT_URL not set (history disabled)

**Fix**: Enable history with `QDRANT_URL=http://localhost:6333`

### "Wrong site suggested"

**Context**: Current mappings are curated based on common patterns.

**Workaround**: Use manual `site:` filter in your query.

**Example**: `rust docs site:rust-lang.org` (overrides auto-rewrite)

## Comparing Results

### Before Phase 2

Query: "rust async"
- Mixed results from all sources
- Blogs, forums, docs equally weighted
- Must manually identify official docs

### After Phase 2

Query: "rust async"
- Official docs prioritized (doc.rust-lang.org)
- Stack Overflow weighted higher
- GitHub issues boosted
- Duplicate warning if searched before

**Result**: Better quality, less time searching!

## Best Practices

### 1. Use Natural Queries

❌ Bad: `site:doc.rust-lang.org rust async`
✅ Good: `rust async docs`

Let the rewriter add the site filter - it knows the best sources.

### 2. Check Suggestions

When a query doesn't auto-rewrite, check the suggestions:

```
💡 Query Optimization Tips:
   1. python tutorial site:docs.python.org
   2. python tutorial documentation
```

### 3. Use History

When you see a duplicate warning:

```
⚠️ Similar search found from 2 hours ago
```

Run `research_history` tool with the same topic to see what you already found.

### 4. Combine with MCP Tools

Workflow:
1. `search_web` with natural query (auto-enhanced)
2. Check `research_history` for past findings
3. `scrape_url` on promising results
4. All automatically logged for next time

## Feature Matrix

| Feature | Enabled By Default | Requires QDRANT_URL |
|---------|-------------------|---------------------|
| Query rewriting | ✅ Yes | ❌ No |
| Query suggestions | ✅ Yes | ❌ No |
| Engine weighting | ✅ Yes | ❌ No |
| Duplicate detection | ⚠️ Only with history | ✅ Yes |
| Search history | ⚠️ Only with history | ✅ Yes |

## Getting Help

### Check Documentation

- `PHASE2_SUMMARY.md` - Complete technical documentation
- `README.md` - User guide and examples
- `TESTING_HISTORY.md` - Testing guide (Phase 1)

### Debug Mode

```bash
RUST_LOG=debug \
SEARXNG_URL=http://localhost:8888 \
./target/release/search-scrape-mcp
```

Shows:
- Detected keywords
- Query rewrite decisions
- Duplicate check results
- Engine responses

## Summary

**Phase 2 gives you**:
- ✅ Smarter searches that find docs faster
- ✅ Duplicate warnings to avoid wasted work
- ✅ Better engine prioritization for dev content
- ✅ Helpful suggestions when auto-rewrite doesn't trigger
- ✅ All automatic, no configuration needed

**Just use natural queries and let the system optimize them for you!**
