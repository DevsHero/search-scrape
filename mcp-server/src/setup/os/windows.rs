use crate::setup::os::action_block;
use crate::setup::{CheckStatus, SetupCheck, SetupOptions};

pub async fn check(_options: &SetupOptions) -> Vec<SetupCheck> {
    let mut checks = Vec::new();
    checks.push(check_elevation());
    checks
}

fn check_elevation() -> SetupCheck {
    let elevated = is_elevated::is_elevated();
    if elevated {
        return SetupCheck {
            id: "windows_admin".to_string(),
            title: "Windows privileges (Administrator)".to_string(),
            status: CheckStatus::Pass,
            details: "Process is running with elevated privileges.".to_string(),
            actions: vec![],
        };
    }

    SetupCheck {
        id: "windows_admin".to_string(),
        title: "Windows privileges (Administrator)".to_string(),
        status: CheckStatus::Warn,
        details: "Process is NOT elevated. Some global input hooks may fail depending on policy."
            .to_string(),
        actions: vec![action_block(
            "Run as Administrator",
            vec![
                "Close the current session.".to_string(),
                "Right click Terminal / VS Code → Run as administrator.".to_string(),
                "If restricted by Windows Security policy, ask your admin to allow input hooks."
                    .to_string(),
            ],
            None,
        )],
    }
}
