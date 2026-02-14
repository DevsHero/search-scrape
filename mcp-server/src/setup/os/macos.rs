use crate::setup::os::{action_block, interactive_hint};
use crate::setup::{CheckStatus, SetupCheck, SetupOptions, SetupRunMode};
use rfd::{MessageButtons, MessageDialog, MessageLevel};

use accessibility_sys::{
    kAXTrustedCheckOptionPrompt, AXIsProcessTrusted, AXIsProcessTrustedWithOptions,
};
use core_foundation_sys::base::{kCFAllocatorDefault, CFRelease};
use core_foundation_sys::dictionary::{
    kCFTypeDictionaryKeyCallBacks, kCFTypeDictionaryValueCallBacks, CFDictionaryCreate,
};
use core_foundation_sys::number::kCFBooleanTrue;
use std::ffi::c_void;

pub async fn check(options: &SetupOptions) -> Vec<SetupCheck> {
    let mut checks = Vec::new();
    checks.push(check_accessibility(options));
    checks
}

fn check_accessibility(options: &SetupOptions) -> SetupCheck {
    // We cannot reliably query TCC Accessibility permission without OS APIs.
    // Best-effort behavior:
    // - In --setup mode, offer to open the Accessibility settings pane.
    // - In startup mode, print guidance.
    let settings_url =
        "x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility";

    let trusted_before = unsafe { AXIsProcessTrusted() };

    let details = format!(
        "Global input hooks (kill switch / input locking) require macOS Accessibility permission. {}\n\nNotes:\n- macOS only shows apps in the Accessibility list AFTER they request permission.\n- If ShadowCrawl is launched by VS Code, you may need to enable Accessibility for VS Code as well.",
        interactive_hint(options)
    );

    let actions = vec![action_block(
        "Enable Accessibility permissions",
        vec![
            "Open System Settings → Privacy & Security → Accessibility".to_string(),
            "Enable the toggle for the launching app (VS Code / Terminal)".to_string(),
            "Restart the app after granting permission".to_string(),
        ],
        Some(settings_url.to_string()),
    )];

    if matches!(options.mode, SetupRunMode::SetupFlag) {
        let result = MessageDialog::new()
            .set_level(MessageLevel::Info)
            .set_title("ShadowCrawl Setup (macOS)")
            .set_description(
                "ShadowCrawl uses global input hooks for the emergency kill switch and (optional) input locking.\n\nOpen Accessibility Settings now?",
            )
            .set_buttons(MessageButtons::OkCancel)
            .show();

        if matches!(
            result,
            rfd::MessageDialogResult::Ok | rfd::MessageDialogResult::Yes
        ) {
            let _ = std::process::Command::new("open").arg(settings_url).spawn();

            // Trigger the OS permission prompt. This is the missing piece that makes the
            // requesting executable appear in the Accessibility list.
            // We do this only in explicit --setup mode.
            let _ = request_accessibility_prompt();
        }
    }

    let trusted_after = unsafe { AXIsProcessTrusted() };
    let status = if trusted_after {
        CheckStatus::Pass
    } else if trusted_before {
        // Shouldn't happen, but keep conservative.
        CheckStatus::Warn
    } else {
        CheckStatus::Warn
    };

    SetupCheck {
        id: "macos_accessibility".to_string(),
        title: "macOS Accessibility (input hooks)".to_string(),
        status,
        details: if trusted_after {
            format!("Accessibility permission granted.\n\n{}", details)
        } else {
            details
        },
        actions,
    }
}

fn request_accessibility_prompt() -> bool {
    unsafe {
        // Build a CFDictionary { kAXTrustedCheckOptionPrompt: true }
        // so macOS shows the system prompt and adds the executable to the list.
        let key_ptr = kAXTrustedCheckOptionPrompt as *const c_void;
        let val_ptr = kCFBooleanTrue as *const c_void;
        let keys: [*const c_void; 1] = [key_ptr];
        let values: [*const c_void; 1] = [val_ptr];

        let dict = CFDictionaryCreate(
            kCFAllocatorDefault,
            keys.as_ptr(),
            values.as_ptr(),
            1,
            &kCFTypeDictionaryKeyCallBacks,
            &kCFTypeDictionaryValueCallBacks,
        );

        if dict.is_null() {
            return AXIsProcessTrustedWithOptions(std::ptr::null());
        }

        let trusted = AXIsProcessTrustedWithOptions(dict);
        CFRelease(dict as *const c_void);
        trusted
    }
}
