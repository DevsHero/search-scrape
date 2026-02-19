use super::RustScraper;
use scraper::{Html, Selector};
use tracing::warn;

impl RustScraper {
    /// Detect domain-specific scraping strategy.
    /// Returns `(wait_time_ms, needs_scroll)`.
    pub(super) fn detect_domain_strategy(&self, domain: &Option<String>) -> (u32, bool) {
        if let Some(d) = domain {
            let d = d.to_lowercase();

            // E-commerce sites: longer wait, scroll for reviews/specs
            if d.contains("amazon") || d.contains("ebay") || d.contains("walmart") {
                return (3000, true);
            }

            // Job sites: scroll for full description
            if d.contains("linkedin") || d.contains("indeed") || d.contains("glassdoor") {
                return (2500, true);
            }

            // Real estate: wait for data hydration
            if d.contains("zillow") || d.contains("redfin") || d.contains("realtor") {
                return (3000, false);
            }

            // Publication platforms: scroll for full article
            if d.contains("substack")
                || d.contains("medium")
                || d.contains("dev.to")
                || d.contains("bloomberg")
            {
                return (2000, true);
            }

            // Social / search streams: scroll for more results
            if d.contains("twitter") || d.contains("x.com") {
                return (2500, true);
            }

            // GitHub: careful with rate limits
            if d.contains("github") {
                return (1500, false);
            }
        }

        // Default: moderate wait, no scroll
        (1000, false)
    }

    /// Returns `true` for domains known to be particularly aggressive about
    /// blocking automated scraping (extra stealth care is warranted).
    pub(super) fn is_boss_domain(&self, domain: &Option<String>) -> bool {
        if let Some(d) = domain {
            let d = d.to_lowercase();
            return d.contains("linkedin")
                || d.contains("zillow")
                || d.contains("redfin")
                || d.contains("trulia")
                || d.contains("substack")
                || d.contains("medium")
                || d.contains("bloomberg")
                || d.contains("instagram")
                || d.contains("twitter")
                || d.contains("x.com");
        }
        false
    }

    /// Inspect HTML response body and return a human-readable block reason,
    /// or `None` when the page appears to be legitimate content.
    pub(super) fn detect_block_reason(&self, html: &str) -> Option<&'static str> {
        let lower = html.to_lowercase();
        let html_size = html.len();

        // If we got a huge HTML response (>500 KB), it's probably not a simple
        // block page — block pages are typically small (<50 KB).
        if html_size > 500_000 {
            let preview = &lower[..lower.len().min(10_000)];

            if preview.contains("verify you are human") || preview.contains("please verify you") {
                return Some("Human Verification");
            }
            if preview.contains("access denied")
                || preview.contains("access to this page has been denied")
            {
                return Some("Access Denied");
            }
            if preview.contains("captcha") && preview.matches("captcha").count() > 2 {
                return Some("Captcha");
            }

            warn!(
                "Block-like text detected but HTML is {}KB - treating as success",
                html_size / 1024
            );
            return None;
        }

        // For smaller responses be strict
        if lower.contains("verify you are human") || lower.contains("please verify you") {
            return Some("Human Verification");
        }
        if lower.contains("duckduckgo.com/anomaly.js")
            || lower.contains("/anomaly.js")
            || lower.contains("anomaly-modal")
        {
            return Some("DuckDuckGo Anomaly");
        }
        if lower.contains("access denied") || lower.contains("access to this page has been denied")
        {
            return Some("Access Denied");
        }
        if lower.contains("captcha")
            || lower.contains("are you human")
            || lower.contains("prove you're human")
        {
            return Some("Captcha");
        }
        if lower.contains("bot detected")
            || lower.contains("unusual traffic")
            || lower.contains("automated request")
        {
            return Some("Bot Detected");
        }
        if lower.contains("cf-chl-")
            || lower.contains("cf-turnstile")
            || lower.contains("turnstile")
        {
            return Some("Cloudflare");
        }
        if lower.contains("perimeterx") || lower.contains("px-captcha") {
            return Some("PerimeterX");
        }
        if lower.contains("page crashed") || lower.contains("crashed!") {
            return Some("JS Crash");
        }
        if lower.contains("zillow group is committed to ensuring digital accessibility")
            && html.len() < 5000
        {
            return Some("Zillow Accessibility Block");
        }

        None
    }

    /// Inspect *extracted clean text content* (post-HTML-processing) to detect auth-wall responses.
    ///
    /// Unlike [`detect_block_reason`] which operates on raw HTML, this works on the
    /// cleaned text output and catches HTTP-200 pages that render a login/sign-in form.
    /// These pages return 200 OK but contain no real content — "soft" auth walls.
    ///
    /// Returns a human-readable error message when an auth wall is detected, `None` otherwise.
    pub(super) fn detect_auth_wall(&self, clean_content: &str, url: &str) -> Option<String> {
        let lower = clean_content.to_lowercase();
        let word_count = clean_content.split_whitespace().count();

        // Short content is a prerequisite — legitimate pages rarely have fewer than 80 words.
        let is_short = word_count < 80;

        // High-confidence signals: explicit phrases that only appear on auth-wall pages.
        let high_confidence = lower.contains("sign in to continue")
            || lower.contains("log in to continue")
            || lower.contains("please sign in")
            || lower.contains("please log in")
            || lower.contains("sign in with google")
            || lower.contains("sign in with github")
            || lower.contains("sign in with microsoft")
            || lower.contains("login to continue")
            || lower.contains("you must be logged in to");

        // Low-confidence signals: only meaningful when content is very short.
        let low_confidence_short = is_short
            && ((lower.contains("sign in") && lower.contains("sign up"))
                || (lower.contains("log in") && lower.contains("sign up"))
                || (lower.contains("create an account") && lower.contains("sign in")));

        if !high_confidence && !low_confidence_short {
            return None;
        }

        if url.contains("github.com") {
            Some(concat!(
                "Content restricted by Auth-Wall (GitHub login page detected). ",
                "Try the raw URL (raw.githubusercontent.com) or append ?plain=1 for Markdown files. ",
                "Recommendation: Use HITL (non_robot_search) to login manually.",
            ).to_string())
        } else {
            Some(
                concat!(
                    "Content restricted by Auth-Wall (login/sign-in page detected). ",
                    "Recommendation: Use HITL (non_robot_search) to login manually.",
                )
                .to_string(),
            )
        }
    }

    // ───────────────────────────────────────────────────────────────────────────
    // 🔒 HTML-Level Auth-Wall Detection — Feature 1 Enhancement
    // Operates on raw HTML via DOM selectors BEFORE content extraction.
    // Catches JS-rendered login pages that produce empty clean_content.
    // ───────────────────────────────────────────────────────────────────────────

    /// Inspect **raw HTML** using CSS selector heuristics to detect auth-wall pages.
    ///
    /// This is the pre-extraction counterpart to [`detect_auth_wall`] which works on
    /// clean text.  Running on raw HTML allows detection of JavaScript-rendered login
    /// pages where `clean_content` would otherwise be empty (the text-level check would
    /// produce a false-negative silence).
    ///
    /// A fast string pre-scan gates the expensive DOM parse so that typical pages
    /// (with no password field at all) pay essentially zero extra latency.
    ///
    /// Returns a human-readable reason string, or `None` when no wall is detected.
    pub(super) fn detect_auth_wall_html(&self, html: &str, url: &str) -> Option<String> {
        // ⚡ Fast gate: parse the DOM only when auth signals are present in the first 60 KB.
        let preview_len = html.len().min(60_000);
        let preview_lower = html[..preview_len].to_lowercase();
        let has_signal = preview_lower.contains("password")
            || preview_lower.contains("sign in")
            || preview_lower.contains("sign_in")
            || preview_lower.contains("log in")
            || preview_lower.contains("login")
            || preview_lower.contains("authenticate");
        if !has_signal {
            return None;
        }

        let document = Html::parse_document(html);

        // ─ High-precision CSS selectors — each uniquely identifies a login form ──────
        //
        // Examples:
        //  #login_field  — GitHub's username input
        //  #password     — GitHub / generic password input
        //  .auth-form    — Many OAuth providers
        //  #loginForm    — Common across enterprise apps
        let selectors: &[(&str, &str)] = &[
            ("#login_field", "GitHub username field"),
            ("#user_login", "login username field"),
            ("[name='password']", "password input (name attr)"),
            ("[name='passwd']", "password input (name=passwd)"),
            ("[name='login']", "login username input (name attr)"),
            ("[type='password']", "password input (type=password)"),
            (".auth-form", "auth-form CSS class"),
            (".login-form", "login-form CSS class"),
            (".signin-form", "signin-form CSS class"),
            ("#login-form", "login-form id"),
            ("#sign_in_form", "sign_in_form id"),
            ("#loginForm", "loginForm id"),
            ("#loginform", "loginform id"),
        ];

        for (sel_str, label) in selectors {
            if let Ok(sel) = Selector::parse(sel_str) {
                if document.select(&sel).next().is_some() {
                    return if url.contains("github.com") {
                        Some(format!(
                            "GitHub Auth-Wall detected (DOM: {label}). \
                             This repo is private or your session has expired. \
                             /blob/ pages are already auto-retried via raw.githubusercontent.com. \
                             Recommendation: Use HITL (non_robot_search) to login manually."
                        ))
                    } else {
                        Some(format!(
                            "Auth-Wall detected (DOM selector matched: {label}). \
                             This page requires authentication before content is served. \
                             Recommendation: Use HITL (non_robot_search) to login manually."
                        ))
                    };
                }
            }
        }

        // ─ Form action pattern check ————————————————————————————————
        // Forms with action='/login', '/session', '/signin' etc. are strong
        // indicators of a dedicated authentication page.
        let auth_form_actions = [
            "/login",
            "/signin",
            "/sign_in",
            "/session",
            "/auth/login",
            "/account/login",
            "/users/sign_in",
        ];
        if let Ok(form_sel) = Selector::parse("form") {
            for form in document.select(&form_sel) {
                if let Some(action) = form.value().attr("action") {
                    let act = action.to_lowercase();
                    if auth_form_actions
                        .iter()
                        .any(|a| act.ends_with(a) || act.contains(a))
                    {
                        return Some(format!(
                            "Auth-Wall detected (login form: action=\"{action}\"). \
                             Recommendation: Use HITL (non_robot_search) to login manually."
                        ));
                    }
                }
            }
        }

        // ─ Page `<title>` check ——————————————————————————————————
        // A page titled "Sign in · GitHub", "Log in | Acme Corp" etc. is
        // almost certainly a pure auth wall with no real content.
        if let Ok(title_sel) = Selector::parse("title") {
            if let Some(el) = document.select(&title_sel).next() {
                let t = el.text().collect::<String>();
                let tl = t.trim().to_lowercase();
                if tl.starts_with("sign in")
                    || tl.starts_with("log in")
                    || tl.starts_with("login")
                    || tl.ends_with("- sign in")
                    || tl.ends_with("· sign in")
                    || tl.ends_with("- log in")
                    || tl.ends_with("· log in")
                {
                    return Some(format!(
                        "Auth-Wall detected (page title: \"{}\"). \
                         Recommendation: Use HITL (non_robot_search) to login manually.",
                        t.trim()
                    ));
                }
            }
        }

        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scraper() -> RustScraper {
        RustScraper::new()
    }

    #[test]
    fn test_detect_auth_wall_explicit_phrase() {
        let s = scraper();
        assert!(s
            .detect_auth_wall(
                "Please sign in to continue viewing this page.",
                "https://example.com"
            )
            .is_some());
        assert!(s
            .detect_auth_wall(
                "Sign in with Google to access your account.",
                "https://example.com"
            )
            .is_some());
    }

    #[test]
    fn test_detect_auth_wall_short_content_both_signals() {
        let s = scraper();
        // Short page with both "sign in" and "sign up" — low-confidence but matches
        let short = "Sign in Sign up to get started today.";
        assert!(s
            .detect_auth_wall(short, "https://app.example.com")
            .is_some());
    }

    #[test]
    fn test_detect_auth_wall_no_false_positives() {
        let s = scraper();
        let real_page = "Rust is a systems programming language focused on three goals: safety, speed, and concurrency. It accomplishes these goals without a garbage collector, making it useful for a number of use cases other languages aren't good at.";
        assert!(s
            .detect_auth_wall(real_page, "https://doc.rust-lang.org")
            .is_none());
    }

    #[test]
    fn test_detect_auth_wall_github_recommendation() {
        let s = scraper();
        let msg = s.detect_auth_wall("Please sign in to continue", "https://github.com/user/repo");
        assert!(msg.is_some());
        let msg = msg.unwrap();
        assert!(msg.contains("raw.githubusercontent.com") || msg.contains("?plain=1"));
        assert!(msg.contains("HITL"));
    }

    // ──────── detect_auth_wall_html tests ────────────────────────────────────

    #[test]
    fn test_detect_auth_wall_html_github_login_field() {
        let s = scraper();
        let html = r#"<html><body>
            <form method="post" action="/session">
              <input type="text" id="login_field" name="login" />
              <input type="password" id="password" name="password" />
              <button type="submit">Sign in</button>
            </form>
        </body></html>"#;
        let result = s.detect_auth_wall_html(html, "https://github.com/login");
        assert!(
            result.is_some(),
            "Should detect GitHub login form via #login_field"
        );
        let msg = result.unwrap();
        assert!(msg.contains("GitHub"), "Should mention GitHub specifically");
        assert!(msg.contains("HITL"));
    }

    #[test]
    fn test_detect_auth_wall_html_type_password_input() {
        let s = scraper();
        let html = r#"<html><body>
            <form action="/auth/login">
              <input type="text" name="username" />
              <input type="password" name="passwd" />
            </form>
        </body></html>"#;
        let result = s.detect_auth_wall_html(html, "https://app.example.com");
        assert!(
            result.is_some(),
            "Should detect password input via type=password"
        );
    }

    #[test]
    fn test_detect_auth_wall_html_form_action_match() {
        let s = scraper();
        // No typed password field here — only form action should trigger detection
        let html = r#"<html><body>
            <form action="/users/sign_in" method="post">
              <input type="text" name="email" />
              <input type="text" name="token" />
            </form>
        </body></html>"#;
        let result = s.detect_auth_wall_html(html, "https://gitlab.com");
        assert!(result.is_some(), "Should detect /users/sign_in form action");
        let msg = result.unwrap();
        assert!(
            msg.contains("login form") || msg.contains("sign_in"),
            "Message should mention the form action: got: {msg}"
        );
    }

    #[test]
    fn test_detect_auth_wall_html_title_sign_in() {
        let s = scraper();
        let html = r#"<html><head><title>Sign in · GitHub</title></head>
        <body><p>Enter your credentials to log in.</p></body></html>"#;
        let result = s.detect_auth_wall_html(html, "https://github.com/login");
        assert!(
            result.is_some(),
            "Should detect auth wall from page title 'Sign in · GitHub'"
        );
    }

    #[test]
    fn test_detect_auth_wall_html_no_false_positive_on_docs() {
        let s = scraper();
        let html = r#"<html><head><title>Rust Documentation</title></head>
        <body>
            <h1>Getting Started</h1>
            <p>Rust is a systems programming language run on every platform.
               The main goal is safety and performance.</p>
            <table>
                <tr><td>Feature</td><td>Description</td></tr>
            </table>
        </body></html>"#;
        let result = s.detect_auth_wall_html(html, "https://doc.rust-lang.org");
        assert!(
            result.is_none(),
            "Clean docs page should not trigger auth-wall detection"
        );
    }

    #[test]
    fn test_detect_auth_wall_html_fast_gate_skips_parse() {
        // Page with no auth-related keywords — fast gate should skip DOM parse
        let s = scraper();
        let html = "<html><body><h1>Hello World</h1><p>Welcome to our site.</p></body></html>";
        let result = s.detect_auth_wall_html(html, "https://example.com");
        assert!(result.is_none());
    }
}
