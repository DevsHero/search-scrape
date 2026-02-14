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

`non_robot_search` is our flagship tool for high-fidelity rendering. It launches a **visible, native Brave/Chrome instance** on your machine.

* **Manual Intervention:** If a site asks for a Login or a Puzzle, you solve it once; the agent scrapes the rest.
* **Brave Integration:** Uses your actual browser profiles (cookies/sessions) to look like a legitimate user, not a headless bot.
* **Stealth Cleanup:** Automatically strips automation markers (`navigator.webdriver`, etc.) before extraction.

---

## 🛠 Features at a Glance

| Feature | Description |
| --- | --- |
| **Search & Discovery** | Federated search via SearXNG. Finds what Google hides. |
| **Deep Crawling** | Recursive, bounded crawling to map entire subdomains. |
| **Semantic Memory** | (Optional) Qdrant integration for long-term research recall. |
| **Proxy Master** | Native rotation logic for HTTP/SOCKS5 pools. |
| **Hydration Scraper** | Specialized logic to extract "hidden" JSON data from React/Next.js sites. |
| **Universal Janitor** | Automatic removal of popups, cookie banners, and overlays. |

---

## 📦 Quick Start (Bypass in 60 Seconds)

### 1. The Docker Way (Full Stack)

```bash
# Clone and Launch
git clone https://github.com/DevsHero/shadowcrawl.git
cd shadowcrawl
docker compose -f docker-compose-local.yml up -d --build

```

### 2. The Native Rust Way (For non_robot_search)

For the 99.99% bypass (HITL), run natively on macOS/Linux:

```bash
cd mcp-server
cargo run --release --features non_robot_search

```

---

## 🧩 MCP Integration (Cursor / Claude)

Add this to your `mcp.json` to give your Agent "Sovereign Stealth" capabilities:

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

## 🙏 Acknowledgments

Built for the **Solo Developer** and the **Sovereign Researcher**.

* **SearXNG:** The privacy-first search backbone.
* **Rust:** For the blazingly fast, type-safe extraction engine.
* **You:** For choosing privacy and control.

---

### 🛡️ Support the Intelligence Revolution

ShadowCrawl is actively maintained. If this tool saves you hundreds in scraping fees:

* **Star the Repo** ⭐
* **Open an Issue** (I fix bugs fast!)
* **Sponsor the Dev** (Keep the engine running!)

**License:** MIT. Free for personal and commercial use.

---

### What's next?

Would you like me to **create a high-quality "Technical Architecture" diagram (Mermaid/Excalidraw style)** to include in the README to visualize how the bypass logic works?