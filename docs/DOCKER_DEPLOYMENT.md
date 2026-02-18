# Binary Build and Release Guide (Zero-Docker)

This repo ships **pure binaries** (no Docker images).

GitHub Actions builds cross-platform artifacts and attaches them to GitHub Releases when you push a commit to `main`/`master` with `[build]` in the commit message.

## Release workflow (GitHub Actions)

Workflow: `.github/workflows/release.yml`

Build matrix:

- macOS: x86_64 + arm64
- Windows: x86_64
- Linux: x86_64

Each release attaches archives containing:

- `shadowcrawl` (HTTP server)
- `shadowcrawl-mcp` (MCP stdio server)
- metadata (`LICENSE`, `README.md`, `server.json`)

## Cut a release

1) Bump version in `mcp-server/Cargo.toml`.

2) Ensure `server.json` version matches.

3) Commit with `[build]` and push to `main`/`master`:

```bash
git commit -am "Release v2.3.0 [build]"
git push
```

4) GitHub Actions will automatically:

- create/push tag `v<version>` from `mcp-server/Cargo.toml`
- build binaries for each platform
- create/update the GitHub Release for that tag
- upload the archives as release assets

Note:
- If tag `v<version>` already exists but points to a different commit, the workflow fails fast (bump version and retry).

## Local build (developer)

```bash
cd mcp-server
cargo build --release --bin shadowcrawl --bin shadowcrawl-mcp
```

Optional (HITL / visible browser):

```bash
cd mcp-server
cargo build --release --features non_robot_search --bin shadowcrawl --bin shadowcrawl-mcp
```
