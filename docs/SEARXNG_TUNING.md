# Legacy: Search Tuning (deprecated)

ShadowCrawl now uses a built-in Rust metasearch engine by default (no external search stack required).
This document is kept for historical reference only.

Main legacy config: [searxng/settings.yml](../searxng/settings.yml)

## Goals

- Reduce noisy results
- Reduce engine bans / captchas
- Keep search fast and predictable for agent workflows

## Key knobs in this repo

### Engine selection (quality vs block risk)

In [searxng/settings.yml](../searxng/settings.yml) see `engines:`.

- Fewer engines usually means more stability.
- Adding more engines increases coverage but increases variance and failure modes.

### `use_mobile_ui: true` (Google)

In [searxng/settings.yml](../searxng/settings.yml) the Google engine is configured with:

- `use_mobile_ui: true`

This can reduce some UI/JS friction and sometimes reduces blocks, at the cost of:
- slightly different snippets
- sometimes less rich result metadata

If you see worse relevance or unexpected snippets, try toggling it:

```yaml
  - name: google
    engine: google
    use_mobile_ui: false
```

Restart the legacy search service after changes.

### Safe search & language

In [searxng/settings.yml](../searxng/settings.yml):

- `search.safe_search`
- `search.default_lang`

If you get irrelevant locales, set `default_lang` to your target.

### Ban/backoff behavior

In [searxng/settings.yml](../searxng/settings.yml):

- `search.ban_time_on_fail`
- `search.max_ban_time_on_fail`
- `search.suspended_times.*`

These settings control how long the legacy search service suspends engines after:
- access denied
- captcha
- too many requests

If you see frequent engine flapping (enabled/disabled), increasing suspension times can make behavior more stable.

### Timeouts

In [searxng/settings.yml](../searxng/settings.yml):

- `outgoing.request_timeout`
- `outgoing.max_request_timeout`

Lower timeouts = faster failures, but can reduce success rate.
Higher timeouts = more results, but more latency.

## Workflow for tuning

1. Change [searxng/settings.yml](../searxng/settings.yml)
2. Restart stack:

```bash
docker compose -f docker-compose-local.yml up -d --build
```

3. Re-run release validation:

```bash
curl -fsS http://localhost:5001/health
curl -fsS http://localhost:5001/mcp/tools | head
```

4. (Optional) Run the Rust test suite:

```bash
cd mcp-server
cargo test
```

## Common symptoms

- "No results" or very few results: engine suspended or blocked
- Frequent captchas: reduce engine count, toggle `use_mobile_ui`, increase suspension time
- Very noisy results: remove low-quality engines and reduce community sources
