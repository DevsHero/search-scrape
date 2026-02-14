use crate::setup::os::{action_block, interactive_hint};
use crate::setup::{CheckStatus, SetupCheck, SetupOptions, SetupRunMode};
use std::path::Path;

pub async fn check(options: &SetupOptions) -> Vec<SetupCheck> {
    let mut checks = Vec::new();
    checks.push(check_display_env());
    checks.push(check_input_devices_access(options));
    checks
}

fn check_display_env() -> SetupCheck {
    let display = std::env::var("DISPLAY").ok();
    let wayland = std::env::var("WAYLAND_DISPLAY").ok();
    let session = std::env::var("XDG_SESSION_TYPE").ok();

    if display.is_some() || wayland.is_some() {
        return SetupCheck {
            id: "linux_display".to_string(),
            title: "Linux display environment".to_string(),
            status: CheckStatus::Pass,
            details: format!(
                "Session: {:?}, DISPLAY={:?}, WAYLAND_DISPLAY={:?}",
                session, display, wayland
            ),
            actions: vec![],
        };
    }

    SetupCheck {
        id: "linux_display".to_string(),
        title: "Linux display environment".to_string(),
        status: CheckStatus::Warn,
        details: "No DISPLAY/WAYLAND_DISPLAY detected (likely SSH/headless). Visible-browser HITL features may not work.".to_string(),
        actions: vec![action_block(
            "Run with a desktop session",
            vec![
                "If using SSH, enable X11 forwarding or run locally in a desktop session.".to_string(),
                "For containers, pass through display (X11/Wayland) if you need visible browsing.".to_string(),
            ],
            None,
        )],
    }
}

fn check_input_devices_access(options: &SetupOptions) -> SetupCheck {
    // A common blocker for global input hooks on Linux is lack of read access to /dev/input.
    // We do a conservative check: if /dev/input exists, ensure at least one event device is readable.
    let dev_input = Path::new("/dev/input");
    if !dev_input.exists() {
        return SetupCheck {
            id: "linux_input".to_string(),
            title: "Linux input hooks (/dev/input)".to_string(),
            status: CheckStatus::Skip,
            details: "/dev/input not present on this system.".to_string(),
            actions: vec![],
        };
    }

    let readable = std::fs::read_dir(dev_input)
        .ok()
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| e.path().to_str().map(|s| s.to_string()))
        .filter(|p| p.contains("event"))
        .take(5)
        .any(|p| std::fs::File::open(&p).is_ok());

    if readable {
        return SetupCheck {
            id: "linux_input".to_string(),
            title: "Linux input hooks (/dev/input)".to_string(),
            status: CheckStatus::Pass,
            details: "At least one /dev/input/event* device is readable.".to_string(),
            actions: vec![],
        };
    }

    let details = format!(
        "No readable /dev/input/event* devices found. Global input hooks may fail. {}",
        interactive_hint(options)
    );

    let mut steps = vec![
        "Add your user to the input group (distro dependent): `sudo usermod -aG input $USER`"
            .to_string(),
        "Log out and back in (or reboot).".to_string(),
        "On Wayland, global hooks may be restricted; consider X11 session for full control."
            .to_string(),
    ];

    if matches!(options.mode, SetupRunMode::SetupFlag) {
        steps.push("Re-run `shadowcrawl --setup` to confirm.".to_string());
    }

    SetupCheck {
        id: "linux_input".to_string(),
        title: "Linux input hooks (/dev/input)".to_string(),
        status: CheckStatus::Warn,
        details,
        actions: vec![action_block("Enable input device access", steps, None)],
    }
}
