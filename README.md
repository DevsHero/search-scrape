# 🥷 ShadowCrawl MCP

<div align="center">
<img src="media/logo.svg" alt="ShadowCrawl Logo" width="180">
<h3><b>Bypass Anything. Scrape Everything.</b></h3>
<p><b>The 99.99% Success Rate Stealth Engine for AI Agents</b></p>
<p><i>The Sovereign, Self-Hosted Alternative to Firecrawl, Jina, and Tavily.</i></p>
</div>

---

**ShadowCrawl** is not just a scraper—it's a **Cyborg Intelligence Layer**. While other APIs fail against Cloudflare, Akamai, and PerimetterX, ShadowCrawl leverages a unique **Human-AI Collaboration** model to achieve a near-perfect bypass rate on even the most guarded "Boss Level" sites (LinkedIn, Airbnb, Ticketmaster).

### 🚀 Why ShadowCrawl?

* **99.99% Bot Bypass:** Featuring the **"Non-Robot Search"** engine. When automation hits a wall, ShadowCrawl bridges the gap with **Human-In-The-Loop (HITL)** interaction, allowing you to solve CAPTCHAs and login walls manually while the agent continues its work.
* **Total Sovereignty:** 100% Private. Self-hosted via Docker. No API keys, no monthly fees, and no third-party data tracking.
* **Agent-Native (MCP):** Deeply integrated with **Cursor, Claude Desktop, and IDEs** via the Model Context Protocol. Your AI agent now has eyes and hands in the real web.
* **Universal Noise Reduction:** Advanced Rust-based filtering that collapses "Skeleton Screens" and repeats, delivering clean, semantic Markdown that reduces LLM token costs.

---

## 💎 The "Nuclear Option": Non-Robot Search (HITL)

Most scrapers try to "act" like a human and fail. ShadowCrawl **uses a human** when it matters.

`non_robot_search` is our flagship tool for high-fidelity rendering. It launches a **visible, native Brave Browser instance** on your machine.

* **Manual Intervention:** If a site asks for a Login or a Puzzle, you solve it once; the agent scrapes the rest.
* **Brave Integration:** Uses your actual browser profiles (cookies/sessions) to look like a legitimate user, not a headless bot.
* **Stealth Cleanup:** Automatically strips automation markers (`navigator.webdriver`, etc.) before extraction.

---

### 💥 Shattering the "Unscrapable" (Anti-Bot Bypass)

Most scraping APIs surrender when facing enterprise-grade shields. ShadowCrawl is the **Hammer** that breaks through. We successfully bypass and extract data from:

* **Cloudflare** 🛡️ (Turnstile / Challenge Pages)
* **DataDome** 🤖 (Interstitial & Behavioral blocks)
* **Akamai** 🏰 (Advanced Bot Manager)
* **PerimeterX / HUMAN** 👤
* **Kasada & Shape Security** 🔐

**The Secret?** The **Cyborg Approach (HITL)**. ShadowCrawl doesn't just "imitate" a human—it bridges your real, native Brave/Chrome session into the agent's workflow. If a human can see it, ShadowCrawl can scrape it.

---

### 📂 Verified Evidence (Boss-Level Targets)

We don't just claim to bypass—we provide the receipts. All evidence below was captured using `non_robot_search` (feature flag: `non_robot_search`) with the Safety Kill Switch enabled (2026-02-14).

| Target Site | Protection | Evidence Size | Data Extracted | Status |
|-------------|-----------|---------------|----------------|--------|
| **LinkedIn** | Cloudflare + Auth | 413KB | [📄 JSON](proof/linkedin_evidence.json) · [📝 Snippet](proof/linkedin_raw_snippet.txt) | 60+ job IDs, listings ✅ |
| **Ticketmaster** | Cloudflare Turnstile | 1.1MB | [📄 JSON](proof/ticketmaster_evidence.json) · [📝 Snippet](proof/ticketmaster_raw_snippet.txt) | Tour dates, venues ✅ |
| **Airbnb** | DataDome | 1.8MB | [📄 JSON](proof/airbnb_evidence.json) · [📝 Snippet](proof/airbnb_raw_snippet.txt) | 1000+ Tokyo listings ✅ |
| **Upwork** | reCAPTCHA | 300KB | [📄 JSON](proof/upwork_evidence.json) · [📝 Snippet](proof/upwork_raw_snippet.txt) | 160K+ job postings ✅ |
| **Amazon** | AWS Shield | 814KB | [📄 JSON](proof/amazon_evidence.json) · [📝 Snippet](proof/amazon_raw_snippet.txt) | RTX 5070 Ti results ✅ |
| **nowsecure.nl** | Cloudflare | 168KB | [📄 JSON](proof/nowsecure_evidence.json) · [📸 Screenshot](proof/Screenshot-nowsecure.png) | Manual button tested ✅ |

> **📖 Full Documentation**: See [proof/README.md](proof/README.md) for verification steps, protection analysis, and quality metrics.


---


## 🛠 Features at a Glance

| Feature | Description |
| --- | --- |
| **Search & Discovery** | Federated search via SearXNG. Finds what Google hides. |
| **Deep Crawling** | Recursive, bounded crawling to map entire subdomains. |
| **Semantic Memory** | (Optional) Embedded LanceDB + Model2Vec for long-term research recall (no separate DB container). (Rust: https://github.com/MinishLab/model2vec-rs) |
| **Proxy Master** | Native rotation logic for HTTP/SOCKS5 pools. |
| **Hydration Scraper** | Specialized logic to extract "hidden" JSON data from React/Next.js sites. |
| **Universal Janitor** | Automatic removal of popups, cookie banners, and overlays. |

---

## 🏆 Comparison

| Feature | Firecrawl / Jina | ShadowCrawl |
| --- | --- | --- |
| **Cost** | Monthly Subscription | **$0 (Self-hosted)** |
| **Privacy** | They see your data | **100% Private** |
| **LinkedIn/Airbnb** | Often Blocked | **99.99% Success (via HITL)** |
| **JS Rendering** | Cloud-only | **Native Brave / Browserless** |
| **Memory** | None | **Semantic Research History** |

---

## 📦 Quick Start (Bypass in 60 Seconds)

### 1. The Docker Way (Full Stack)

Docker is the fastest way to bring up the full stack (SearXNG, proxy manager, etc.).

**Important:** Docker mode cannot use the HITL/GUI renderer (`non_robot_search`) because containers cannot reliably access your host's native Brave/Chrome window, keyboard hooks, and OS permissions.
Use the **Native Rust Way** below when you want boss-level bypass.

```bash
# Clone and Launch
git clone https://github.com/DevsHero/shadowcrawl.git
cd shadowcrawl
docker compose -f docker-compose-local.yml up -d --build

```
### 2. Quick build (all features, recommended):**

```bash
cd mcp-server
cargo build --release --all-features
```


This produces the local MCP binary at:

- `mcp-server/target/release/shadowcrawl-mcp`

Prereqs:

- Install Brave Browser (recommended) or Google Chrome
- Grant Accessibility permissions (required for the emergency ESC hold-to-abort kill switch)

Windows:
- Setup guide: `docs/window_setup.md`
Ubuntu:
- Setup guide: `docs/ubuntu_setup.md`

---

## 🧩 MCP Integration (Cursor / Claude / VS Code)

ShadowCrawl can run as an MCP server in 2 modes:

- **Docker MCP server**: great for normal scraping/search tools, but **cannot** do HITL/GUI (`non_robot_search`).
- **Local MCP server (`shadowcrawl`)**: required for HITL tools (a visible Brave/Chrome window).

### Option A: Docker MCP server (no non_robot_search)

Add this to your MCP config to use the Dockerized server:

```json
{
  "mcpServers": {
    "shadowcrawl": {
      "command": "docker",
      "args": [
        "compose",
        "-f",
        "/YOUR_PATH/shadowcrawl/docker-compose-local.yml",
        "exec",
        "-i",
        "-T",
        "shadowcrawl",
        "shadowcrawl-mcp"
      ]
    }
  }
}

```

### Option B: Local MCP server (required for non_robot_search)

If you want to use HITL tools like `non_robot_search`, configure a **local** MCP server that launches the native binary.

VS Code MCP config example ("servers" format):

```jsonc
{
  "servers": {
    "shadowcrawl": {
      "type": "stdio",
      "command": "env",
      "args": [
        "RUST_LOG=info",

        // Optional (only if you run the full stack locally):
        "SEARXNG_URL=http://localhost:8890",
        "BROWSERLESS_URL=http://localhost:3010",
        "BROWSERLESS_TOKEN=mcp_stealth_session",
        // Optional semantic memory (embedded LanceDB on local filesystem):
        "LANCEDB_URI=/YOUR_PATH/shadowcrawl/lancedb",

        // Note: Qdrant is no longer used. Remove any legacy `QDRANT_URL=...` from your MCP config.

        // Optional: choose a Model2Vec model (HF repo id or local path)
        // "MODEL2VEC_MODEL=minishlab/potion-base-8M",

        // Network + limits:
        "HTTP_TIMEOUT_SECS=30",
        "HTTP_CONNECT_TIMEOUT_SECS=10",
        "OUTBOUND_LIMIT=32",
        "MAX_CONTENT_CHARS=10000",
        "MAX_LINKS=100",

        // Optional (proxy manager):
        "IP_LIST_PATH=/YOUR_PATH/shadowcrawl/ip.txt",
        "PROXY_SOURCE_PATH=/YOUR_PATH/shadowcrawl/proxy_source.json",

        // HITL / non_robot_search quality-of-life:
        // "SHADOWCRAWL_NON_ROBOT_AUTO_ALLOW=1",
        // "SHADOWCRAWL_RENDER_PROFILE_DIR=/YOUR_PROFILE_DIR",
        // "CHROME_EXECUTABLE=/Applications/Brave Browser.app/Contents/MacOS/Brave Browser",

        "/YOUR_PATH/shadowcrawl/mcp-server/target/release/shadowcrawl-mcp"
      ]
    }
  }
}
```

Notes:

- MCP tool name: **`non_robot_search`** (internal handler + feature flag name: `non_robot_search`).
- For HITL, prefer Brave + a real profile dir (`SHADOWCRAWL_RENDER_PROFILE_DIR`) so cookies/sessions persist.
- If you're running via Docker MCP server, HITL tools will either be unavailable or fail (no host GUI).

---

### ☕ Acknowledgments & Support

ShadowCrawl is built with ❤️ by a **Solo Developer** for the open-source community. If this tool helped you bypass a $500/mo API, consider supporting its growth!

* **Found a bug?** [Open an Issue](https://github.com/DevsHero/shadowcrawl/issues).
* **Want a feature?** Submit a request!
* **Love the project?** Star the repo ⭐ or buy me a coffee to fuel more updates!

[![Sponsor](https://img.shields.io/static/v1?label=Sponsor&message=%E2%9D%A4&logo=GitHub&color=ff69b4&style=for-the-badge)](https://github.com/sponsors/DevsHero)

**License:** MIT. Free for personal and commercial use.

---
 
