use crate::setup::{ActionRequired, SetupCheck, SetupOptions, SetupRunMode};

#[cfg(all(unix, not(target_os = "macos")))]
mod linux;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "windows")]
mod windows;

#[cfg(target_os = "macos")]
pub async fn check_permissions(options: &SetupOptions) -> Vec<SetupCheck> {
    macos::check(options).await
}

#[cfg(target_os = "windows")]
pub async fn check_permissions(options: &SetupOptions) -> Vec<SetupCheck> {
    windows::check(options).await
}

#[cfg(all(unix, not(target_os = "macos")))]
pub async fn check_permissions(options: &SetupOptions) -> Vec<SetupCheck> {
    linux::check(options).await
}

#[cfg(not(any(
    target_os = "macos",
    target_os = "windows",
    all(unix, not(target_os = "macos"))
)))]
pub async fn check_permissions(_options: &SetupOptions) -> Vec<SetupCheck> {
    vec![SetupCheck {
        id: "os_permissions".to_string(),
        title: "OS permissions".to_string(),
        status: crate::setup::CheckStatus::Skip,
        details: "Unsupported OS for permission checks.".to_string(),
        actions: vec![ActionRequired {
            title: "Proceed with caution".to_string(),
            steps: vec![
                "This platform isn't currently supported by shadow-setup checks.".to_string(),
                "Run `--setup` on a supported desktop OS for full HITL features.".to_string(),
            ],
            open_url: None,
        }],
    }]
}

pub(crate) fn interactive_hint(options: &SetupOptions) -> String {
    match options.mode {
        SetupRunMode::Startup => {
            "(not verified during startup; run with --setup for interactive checks)".to_string()
        }
        SetupRunMode::SetupFlag => "".to_string(),
    }
}

pub(crate) fn action_block(
    title: &str,
    steps: Vec<String>,
    open_url: Option<String>,
) -> ActionRequired {
    ActionRequired {
        title: title.to_string(),
        steps,
        open_url,
    }
}
