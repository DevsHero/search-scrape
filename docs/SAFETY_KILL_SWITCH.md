# Safety Kill Switch - Implementation Details

## Overview

The Safety Kill Switch is a comprehensive browser process management system that prevents ShadowCrawl from hanging after successful scrapes, especially on sites with persistent background scripts (Cloudflare, DataDome, etc.).

## Problem Statement

During high-fidelity scraping (`non_robot_search`/`non_robot_search`), the browser process would sometimes fail to terminate after successful content extraction. This was particularly common on:
- Cloudflare-protected sites (nowsecure.nl)
- DataDome-protected sites (Zillow, Airbnb)
- LinkedIn job postings

The terminal would show "Success!" but remain hung, requiring manual intervention.

## Solution Architecture

### 1. Global Timeout Wrapper

**File**: `mcp-server/src/features/non_robot_search.rs`

**Implementation**:
```rust
async fn execute_non_robot_search_impl(
    state: &Arc<AppState>,
    cfg: NonRobotSearchConfig,
) -> Result<ScrapeResponse, NonRobotSearchError> {
    // Global timeout: human_timeout + 30s safety margin
    // HITL no longer uses a strict `tokio::time::timeout` wrapper.
    // The visible browser should remain open until the operator explicitly
    // clicks "FINISH & RETURN" in the in-browser overlay.
    execute_non_robot_search_inner(state, cfg).await
}
```

**Behavior**:
- HITL waits for explicit user termination via **FINISH & RETURN**
- `human_timeout_seconds` is treated as a **soft** window (used for warnings / operator expectations), not as a kill deadline
- A periodic heartbeat keeps the browser/CDP transport active while waiting

**Why it matters**: Real-world logins (2FA / OAuth / CAPTCHA) can take minutes. The system should not close the headed browser until the user confirms completion.

---

### 2. Force-Kill Helper Function

**Function**: `force_kill_all_debug_browsers(debugging_port: u16)`

**Implementation**:
```rust
fn force_kill_all_debug_browsers(debugging_port: u16) {
    // Aggressively kill ALL browser processes using this debugging port
    let marker = format!("--remote-debugging-port={}", debugging_port);

    // Use ps to find matching processes
    let ps = std::process::Command::new("ps")
        .args(["-ax", "-o", "pid=,command="])
        .output();
    
    // Parse output and kill matching PIDs
    for line in text.lines() {
        if cmd.contains(&marker) {
            // Immediate SIGKILL (-9) for force cleanup
            let _ = std::process::Command::new("kill")
                .arg("-9")
                .arg(pid.to_string())
                .status();
        }
    }
}
```

**Behavior**:
- Finds ALL processes with `--remote-debugging-port=9222` in their command line
- Sends SIGKILL (-9) immediately (no graceful termination)
- Logs the number of processes killed

**Difference from `kill_debug_browser_zombies`**:
| Function | Purpose | Signal | When Used |
|----------|---------|--------|-----------|
| `kill_debug_browser_zombies` | Preventive cleanup before launch | SIGTERM (-15), then SIGKILL if alive | Before browser launch |
| `force_kill_all_debug_browsers` | Emergency cleanup | SIGKILL (-9) immediately | After timeout or on Drop |

---

### 3. Drop Trait Implementation

**Implementation**:
```rust
impl Drop for BrowserSession {
    fn drop(&mut self) {
        info!("BrowserSession drop - force-killing browser on port {}", self.debugging_port);
        force_kill_all_debug_browsers(self.debugging_port);
        
        // Clean up temp profile if created
        if self.created_profile_dir {
            if let Some(dir) = self.profile_dir.take() {
                let _ = std::fs::remove_dir_all(dir);
            }
        }
    }
}
```

**Behavior**:
- Automatically called when `BrowserSession` goes out of scope
- Ensures browser processes are killed even if:
  - The code panics
  - Early return happens
  - Timeout is reached
- Prevents zombie processes from accumulating

**Why it's critical**: Rust's RAII (Resource Acquisition Is Initialization) guarantees that Drop is called, making this the last line of defense.

---

### 4. Enhanced `BrowserSession::close()`

**Implementation**:
```rust
async fn close(&mut self) {
    info!("closing browser session (port {})", self.debugging_port);
    
    // 1. Close tabs first (reduces "Restore tabs?" prompt)
    let _ = close_all_tabs_via_json(self.debugging_port).await;

    // 2. Close CDP connection gracefully
    let _ = self.browser.close().await;
    let _ = self.browser.wait().await;
    self.handler_task.abort();
    
    // 3. Wait briefly for graceful shutdown
    tokio::time::sleep(Duration::from_millis(500)).await;
    
    // 4. Force-kill any remaining browser processes
    force_kill_all_debug_browsers(self.debugging_port);

    // 5. Clean up temp profiles
    if self.created_profile_dir {
        if let Some(dir) = self.profile_dir.take() {
            let _ = std::fs::remove_dir_all(dir);
        }
    }
}
```

**Behavior**:
1. **Graceful shutdown first**: Close tabs → Close CDP → Abort handler
2. **Grace period**: Wait 500ms for browser to exit cleanly
3. **Force cleanup**: Kill any remaining processes
4. **Profile cleanup**: Remove temporary directories

**Why both graceful and force?**:
- Graceful shutdown avoids "Restore tabs?" prompts in Brave
- Force cleanup ensures no processes are left behind

---

## Execution Flow

```
User calls non_robot_search
    ↓
execute_non_robot_search_impl()
    ↓ (wrapped in tokio::time::timeout)
execute_non_robot_search_inner()
    ↓
BrowserSession::launch()
    ↓
run_flow() - Navigate, detect challenges, extract content
    ↓
session.close() - Graceful shutdown + force-kill
    ↓
Drop trait - Final cleanup guarantee
    ↓
return ScrapeResponse
    ↓ (timeout elapsed?)
force_kill_all_debug_browsers(9222) - Emergency cleanup
```

## Verification

### Test Scenario

**Target**: Cloudflare-protected site (https://nowsecure.nl)

**Before Fix**:
- Browser launches ✅
- Content extraction succeeds ✅
- Output shows "Success!" ✅
- Terminal hangs indefinitely ❌
- Browser processes remain running ❌

**After Fix**:
- Browser launches ✅
- Content extraction succeeds ✅
- Output shows "Success!" ✅
- Terminal returns immediately ✅
- No browser processes remain ✅

### Verification Commands

```bash
# Check for hanging browsers
ps aux | grep -E "(Brave|Chrome)" | grep -E "remote-debugging"

# Should return empty after scrape completes
```

## Configuration

### Environment Variables

| Variable | Purpose | Default |
|----------|---------|---------|
| `SHADOWCRAWL_NON_ROBOT_AUTO_ALLOW` | Skip consent prompt | `false` |
| `SHADOWCRAWL_NON_ROBOT_CONSENT` | Consent mode (tty/dialog/auto) | `auto` |
| `CHROME_EXECUTABLE` | Browser path override | Auto-detect |

### Timeout Calculation

```rust
human_timeout_seconds = soft wait window (no forced browser close)
```

**Example**:
- User sets `human_timeout_seconds: 120` (2 minutes for HITL)
- Global timeout = 120 + 30 = **150 seconds** (2.5 minutes)
- After 150s, force-kill is triggered regardless of state

## Edge Cases Handled

1. **Browser crashes mid-scrape**
   - Drop trait ensures cleanup
   - Global timeout prevents infinite wait

2. **User closes browser manually**
   - `is_closed()` check detects disconnect
   - Returns `BrowserClosed` error gracefully

3. **Network stalls during extraction**
   - Global timeout triggers emergency cleanup
   - Returns timeout error with diagnostic info

4. **Multiple rapid calls**
   - `non_robot_search_lock` serializes execution
   - Prevents profile lock conflicts

5. **Process panics**
   - Drop trait runs before unwinding
   - No zombie browsers left behind

## Performance Impact

- **Graceful shutdown**: 500ms overhead per scrape
- **Force-kill check**: <50ms (ps + kill commands)
- **Drop trait**: ~10ms (process enumeration)

**Total**: <600ms cleanup overhead per scrape (acceptable for 2+ minute scrapes)

## Platform Support

| Platform | Status | Notes |
|----------|--------|-------|
| macOS | ✅ Tested | Primary development platform |
| Ubuntu Desktop / Linux | ⚠️ Best-effort | Works best on X11; Wayland/input permissions may restrict kill switch. See docs/UBUNTU_DESKTOP.md |
| Windows | ✅ Tested | Verified Windows install + HITL setup: docs/WINDOWS_SETUP.md |

## Future Improvements

1. **Cross-platform process management**: Replace `ps`/`kill` with a cross-platform approach (recommended: `sysinfo`) to enumerate processes by cmdline and terminate by PID.
2. **Metrics**: Track force-kill frequency to detect problematic sites
3. **Progressive Timeout**: Start with graceful shutdown, escalate to SIGTERM, then SIGKILL
4. **Health Check**: Add heartbeat mechanism to detect hung browser earlier

## Changelog

### 2026-02-14 - Initial Implementation

- Added global timeout wrapper
- Implemented `force_kill_all_debug_browsers()` helper
- Added Drop trait to `BrowserSession`
- Enhanced `close()` with force-kill fallback
- Verified on Cloudflare-protected site (nowsecure.nl)

---

## Related Documentation

- [Non-Robot Search Guide](NON_ROBOT_SEARCH.md)
- [IDE Setup](IDE_SETUP.md)
- [Binary Release Guide](DOCKER_DEPLOYMENT.md)
