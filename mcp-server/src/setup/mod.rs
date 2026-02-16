use std::fmt;
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tracing::warn;

pub mod os;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SetupRunMode {
    /// Non-interactive (safe to run during normal startup). No dialogs; only logs.
    Startup,
    /// Interactive (invoked explicitly via `--setup`). May show dialogs and open settings.
    SetupFlag,
}

#[derive(Clone, Debug)]
pub struct SetupOptions {
    pub mode: SetupRunMode,
    pub http_port: u16,
    pub ping_target: &'static str,
    pub ping_timeout: Duration,
    pub https_probe_url: &'static str,
}

impl Default for SetupOptions {
    fn default() -> Self {
        Self {
            mode: SetupRunMode::Startup,
            http_port: 5000,
            ping_target: "8.8.8.8",
            ping_timeout: Duration::from_secs(2),
            https_probe_url: "https://example.com",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CheckStatus {
    Pass,
    Warn,
    Fail,
    Skip,
}

impl CheckStatus {
    pub fn is_fail(self) -> bool {
        matches!(self, CheckStatus::Fail)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ActionRequired {
    pub title: String,
    pub steps: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub open_url: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SetupCheck {
    pub id: String,
    pub title: String,
    pub status: CheckStatus,
    pub details: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub actions: Vec<ActionRequired>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct SetupReport {
    pub checks: Vec<SetupCheck>,
}

impl SetupReport {
    pub fn has_failures(&self) -> bool {
        self.checks.iter().any(|c| c.status.is_fail())
    }

    pub fn summarize_for_logs(&self) -> String {
        let mut pass = 0;
        let mut warn_count = 0;
        let mut fail = 0;
        let mut skip = 0;
        for c in &self.checks {
            match c.status {
                CheckStatus::Pass => pass += 1,
                CheckStatus::Warn => warn_count += 1,
                CheckStatus::Fail => fail += 1,
                CheckStatus::Skip => skip += 1,
            }
        }
        format!(
            "setup: {} pass, {} warn, {} fail, {} skip",
            pass, warn_count, fail, skip
        )
    }

    pub fn print_action_required_blocks(&self) {
        for check in &self.checks {
            if check.actions.is_empty() {
                continue;
            }

            warn!(
                "\n=== ACTION REQUIRED: {} ===\n{}\n",
                check.title, check.details
            );
            for action in &check.actions {
                eprintln!("- {}", action.title);
                for step in &action.steps {
                    eprintln!("  • {}", step);
                }
                if let Some(url) = &action.open_url {
                    eprintln!("  • Open: {}", url);
                }
                eprintln!();
            }
        }
    }
}

impl fmt::Display for SetupReport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "ShadowCrawl Pre-flight Checklist")?;
        writeln!(f, "{}", "=".repeat(32))?;
        for c in &self.checks {
            writeln!(
                f,
                "[{:<4}] {}\n  {}",
                match c.status {
                    CheckStatus::Pass => "OK",
                    CheckStatus::Warn => "WARN",
                    CheckStatus::Fail => "FAIL",
                    CheckStatus::Skip => "SKIP",
                },
                c.title,
                c.details.replace('\n', "\n  ")
            )?;
            for action in &c.actions {
                writeln!(f, "  Action: {}", action.title)?;
                for step in &action.steps {
                    writeln!(f, "    - {}", step)?;
                }
                if let Some(url) = &action.open_url {
                    writeln!(f, "    - Open: {}", url)?;
                }
            }
            writeln!(f)?;
        }
        Ok(())
    }
}

pub async fn check_all(options: SetupOptions) -> SetupReport {
    let mut report = SetupReport::default();

    report.checks.push(check_chrome_installed());
    report.checks.push(check_storage_dirs());
    report
        .checks
        .push(check_network_ping(options.ping_target, options.ping_timeout).await);
    report
        .checks
        .push(check_https_tls(options.https_probe_url).await);
    report.checks.push(check_port_available(options.http_port));

    // OS permissions and environment.
    report.checks.extend(os::check_permissions(&options).await);

    report
}

fn check_chrome_installed() -> SetupCheck {
    let candidates = [
        "brave",
        "brave-browser",
        "google-chrome",
        "chrome",
        "chromium",
        "chromium-browser",
        "Microsoft Edge",
    ];

    let mut found = Vec::new();
    for c in candidates {
        if let Ok(path) = which::which(c) {
            found.push(path);
        }
    }

    #[cfg(target_os = "macos")]
    {
        let app_paths = [
            "/Applications/Brave Browser.app/Contents/MacOS/Brave Browser",
            "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
            "/Applications/Chromium.app/Contents/MacOS/Chromium",
            "/Applications/Microsoft Edge.app/Contents/MacOS/Microsoft Edge",
        ];
        for p in app_paths {
            if Path::new(p).exists() {
                found.push(PathBuf::from(p));
            }
        }
    }

    if !found.is_empty() {
        return SetupCheck {
            id: "chrome_installed".to_string(),
            title: "Browser dependency (Brave/Chrome/Chromium)".to_string(),
            status: CheckStatus::Pass,
            details: format!(
                "Found browser executable(s):\n{}",
                found
                    .iter()
                    .take(5)
                    .map(|p| format!("- {}", p.display()))
                    .collect::<Vec<_>>()
                    .join("\n")
            ),
            actions: vec![],
        };
    }

    let actions = vec![ActionRequired {
        title: "Install Brave/Chrome/Chromium".to_string(),
        steps: vec![
            "macOS: install Brave from https://brave.com/download/ (recommended for stealth_scrape / HITL)"
                .to_string(),
            "macOS: install Google Chrome from https://www.google.com/chrome/".to_string(),
            "Ubuntu/Debian: `sudo apt-get update && sudo apt-get install -y chromium-browser`"
                .to_string(),
            "Fedora: `sudo dnf install -y chromium`".to_string(),
            "Arch: `sudo pacman -S chromium`".to_string(),
            "Windows: install Chrome from https://www.google.com/chrome/".to_string(),
        ],
        open_url: Some("https://brave.com/download/".to_string()),
    }];

    SetupCheck {
        id: "chrome_installed".to_string(),
        title: "Browser dependency (Brave/Chrome/Chromium)".to_string(),
        status: CheckStatus::Fail,
        details: "No Brave/Chrome/Chromium executable found on PATH (or common install locations)."
            .to_string(),
        actions,
    }
}

fn check_storage_dirs() -> SetupCheck {
    let Some(home) = dirs::home_dir() else {
        return SetupCheck {
            id: "storage_dirs".to_string(),
            title: "Storage access (~/.shadowcrawl/*)".to_string(),
            status: CheckStatus::Fail,
            details: "Unable to resolve home directory.".to_string(),
            actions: vec![ActionRequired {
                title: "Set HOME and retry".to_string(),
                steps: vec!["Ensure the HOME environment variable is set.".to_string()],
                open_url: None,
            }],
        };
    };

    let base = home.join(".shadowcrawl");
    let data = base.join("data");
    let logs = base.join("logs");

    let mut created = Vec::new();
    for dir in [&base, &data, &logs] {
        if dir.exists() {
            continue;
        }
        if let Err(e) = std::fs::create_dir_all(dir) {
            return SetupCheck {
                id: "storage_dirs".to_string(),
                title: "Storage access (~/.shadowcrawl/*)".to_string(),
                status: CheckStatus::Fail,
                details: format!("Failed to create {}: {}", dir.display(), e),
                actions: vec![ActionRequired {
                    title: "Fix permissions".to_string(),
                    steps: vec![
                        format!("Create directories manually: {}", dir.display()),
                        "Ensure the user running ShadowCrawl has write permission.".to_string(),
                    ],
                    open_url: None,
                }],
            };
        }
        created.push(dir.display().to_string());
    }

    // Writability test: create/delete a small file.
    let probe = data.join(".write_test");
    if let Err(e) = std::fs::write(&probe, b"ok") {
        return SetupCheck {
            id: "storage_dirs".to_string(),
            title: "Storage access (~/.shadowcrawl/*)".to_string(),
            status: CheckStatus::Fail,
            details: format!("Directory not writable: {} ({})", data.display(), e),
            actions: vec![ActionRequired {
                title: "Fix directory permissions".to_string(),
                steps: vec![
                    format!("Ensure writable: {}", data.display()),
                    "Try: `chmod -R u+rwX ~/.shadowcrawl`".to_string(),
                ],
                open_url: None,
            }],
        };
    }
    let _ = std::fs::remove_file(&probe);

    SetupCheck {
        id: "storage_dirs".to_string(),
        title: "Storage access (~/.shadowcrawl/*)".to_string(),
        status: CheckStatus::Pass,
        details: if created.is_empty() {
            format!("Writable: {} and {}", data.display(), logs.display())
        } else {
            format!(
                "Created and verified writable directories:\n{}",
                created
                    .into_iter()
                    .map(|d| format!("- {}", d))
                    .collect::<Vec<_>>()
                    .join("\n")
            )
        },
        actions: vec![],
    }
}

async fn check_network_ping(target: &str, timeout: Duration) -> SetupCheck {
    let target_owned = target.to_string();

    let args: Vec<String> = {
        #[cfg(target_os = "windows")]
        {
            vec![
                "-n".to_string(),
                "1".to_string(),
                "-w".to_string(),
                timeout.as_millis().to_string(),
                target_owned.clone(),
            ]
        }

        #[cfg(not(target_os = "windows"))]
        {
            vec![
                "-c".to_string(),
                "1".to_string(),
                "-W".to_string(),
                timeout.as_secs().to_string(),
                target_owned.clone(),
            ]
        }
    };

    let ping_res = tokio::task::spawn_blocking(move || {
        std::process::Command::new("ping")
            .args(&args)
            .output()
            .map(|o| o.status.success())
    })
    .await
    .ok()
    .and_then(|r| r.ok());

    if ping_res == Some(true) {
        return SetupCheck {
            id: "network_ping".to_string(),
            title: "Network connectivity (ping)".to_string(),
            status: CheckStatus::Pass,
            details: format!("Ping to {} succeeded.", target),
            actions: vec![],
        };
    }

    // Fallback: TCP connect (often allowed even if ICMP blocked).
    let tcp_ok = tokio::net::TcpStream::connect((target, 53)).await.is_ok();

    if tcp_ok {
        return SetupCheck {
            id: "network_ping".to_string(),
            title: "Network connectivity".to_string(),
            status: CheckStatus::Warn,
            details: format!(
                "ICMP ping to {} failed/blocked, but TCP connect to {}:53 succeeded.",
                target, target
            ),
            actions: vec![],
        };
    }

    SetupCheck {
        id: "network_ping".to_string(),
        title: "Network connectivity".to_string(),
        status: CheckStatus::Fail,
        details: format!("Unable to reach {} (ping and TCP fallback failed).", target),
        actions: vec![ActionRequired {
            title: "Check outbound network policies".to_string(),
            steps: vec![
                "Verify you have internet access.".to_string(),
                "If on a corporate network, check firewall/VPN policies.".to_string(),
                "If running in a restricted container, allow outbound traffic.".to_string(),
            ],
            open_url: None,
        }],
    }
}

async fn check_https_tls(url: &str) -> SetupCheck {
    let client = match reqwest::Client::builder()
        .timeout(Duration::from_secs(5))
        .build()
    {
        Ok(c) => c,
        Err(e) => {
            return SetupCheck {
                id: "https_tls".to_string(),
                title: "HTTPS / certificate store".to_string(),
                status: CheckStatus::Warn,
                details: format!("Failed to construct HTTP client: {}", e),
                actions: vec![],
            };
        }
    };

    match client.get(url).send().await {
        Ok(resp) => SetupCheck {
            id: "https_tls".to_string(),
            title: "HTTPS / certificate store".to_string(),
            status: if resp.status().is_success() {
                CheckStatus::Pass
            } else {
                CheckStatus::Warn
            },
            details: format!("HTTPS probe {} returned status {}.", url, resp.status()),
            actions: vec![],
        },
        Err(e) => SetupCheck {
            id: "https_tls".to_string(),
            title: "HTTPS / certificate store".to_string(),
            status: CheckStatus::Fail,
            details: format!("HTTPS probe failed (possible trust store issue): {}", e),
            actions: vec![ActionRequired {
                title: "Fix system certificate store".to_string(),
                steps: vec![
                    "Windows: ensure Windows Update and root certs are up to date.".to_string(),
                    "Linux: install/refresh CA bundle (e.g., `ca-certificates`).".to_string(),
                    "If behind MITM proxy, install enterprise root CA.".to_string(),
                ],
                open_url: None,
            }],
        },
    }
}

fn check_port_available(port: u16) -> SetupCheck {
    let addr = format!("127.0.0.1:{}", port);
    match TcpListener::bind(&addr) {
        Ok(listener) => {
            drop(listener);
            SetupCheck {
                id: "port_conflict".to_string(),
                title: "Port conflict check".to_string(),
                status: CheckStatus::Pass,
                details: format!("Port {} is available.", port),
                actions: vec![],
            }
        }
        Err(e) => SetupCheck {
            id: "port_conflict".to_string(),
            title: "Port conflict check".to_string(),
            status: CheckStatus::Warn,
            details: format!("Port {} is not available: {}", port, e),
            actions: vec![ActionRequired {
                title: "Free the port or change deployment".to_string(),
                steps: vec![
                    format!("Stop the service using port {}.", port),
                    "Or run ShadowCrawl on a different port (if supported by your deployment)."
                        .to_string(),
                ],
                open_url: None,
            }],
        },
    }
}
