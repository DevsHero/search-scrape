# Binary Build and Release Guide (Zero-Docker)

This repo ships **pure binaries** (no Docker images).

Releases are built **locally** and uploaded to GitHub Releases using `scripts/release.sh`.

## Cut a release

1) Bump version in `mcp-server/Cargo.toml`.

2) Ensure `server.json` version matches.

3) Ensure `CHANGELOG.md` has the notes you want under `## Unreleased`.

4) Commit and push (no `[build]` needed):

```bash
git commit -am "Release vX.Y.Z"
git push
```

5) Run the local release script:

```bash
bash scripts/release.sh
```

This script will create/push tag `v<version>`, build cross-platform binaries, and upload artifacts to GitHub Releases.

Note: the script also uses `CHANGELOG.md` → `Unreleased` as the GitHub Release notes.

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
