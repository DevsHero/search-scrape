use super::RustScraper;

impl RustScraper {
    /// Generate Canvas/WebGL spoofing script for God Level domains
    pub(super) fn get_canvas_spoof_script(&self) -> String {
        r#"
// Canvas Fingerprint Spoofing
const originalGetContext = HTMLCanvasElement.prototype.getContext;
HTMLCanvasElement.prototype.getContext = function(type, ...args) {
    const context = originalGetContext.apply(this, [type, ...args]);
    if (type === '2d' || type === 'webgl' || type === 'webgl2') {
        if (context) {
            // Add noise to canvas fingerprinting
            const originalToDataURL = this.toDataURL;
            this.toDataURL = function(...args) {
                const data = originalToDataURL.apply(this, args);
                return data.replace(/.$/, String.fromCharCode(Math.random() * 10 | 0));
            };
        }
    }
    return context;
};

// WebGL Fingerprint Spoofing - Mask "Google SwiftShader"
const getParameter = WebGLRenderingContext.prototype.getParameter;
WebGLRenderingContext.prototype.getParameter = function(parameter) {
    if (parameter === 37445) {  // UNMASKED_VENDOR_WEBGL
        return 'Intel Inc.';
    }
    if (parameter === 37446) {  // UNMASKED_RENDERER_WEBGL
        return 'Intel Iris OpenGL Engine';
    }
    return getParameter.apply(this, arguments);
};

// WebGL2 support
if (typeof WebGL2RenderingContext !== 'undefined') {
    const getParameter2 = WebGL2RenderingContext.prototype.getParameter;
    WebGL2RenderingContext.prototype.getParameter = function(parameter) {
        if (parameter === 37445) return 'Intel Inc.';
        if (parameter === 37446) return 'Intel Iris OpenGL Engine';
        return getParameter2.apply(this, arguments);
    };
}

// Navigator properties spoofing
Object.defineProperty(navigator, 'webdriver', {get: () => false});
Object.defineProperty(navigator, 'plugins', {get: () => [1, 2, 3, 4, 5]});
Object.defineProperty(navigator, 'languages', {get: () => ['en-US', 'en']});
"#
        .to_string()
    }

    /// Universal stealth script for ALL sites - Protocol-level anti-detection
    pub(super) fn get_universal_stealth_script(&self) -> String {
        r#"
// ====== UNIVERSAL STEALTH ENGINE ======
// Injected before page load for ALL sites (site-agnostic)

// 0. Navigator hardening (webdriver + languages) — do this before anything else
(() => {
    try {
        const proto = Navigator.prototype;

        // webdriver: prefer "absent" (undefined) over false
        try {
            Object.defineProperty(proto, 'webdriver', {
                get: () => undefined,
                configurable: true,
            });
        } catch (e) {}
        try { delete navigator.webdriver; } catch (e) {}

        // languages: realistic list
        try {
            Object.defineProperty(proto, 'languages', {
                get: () => ['en-US', 'en'],
                configurable: true,
            });
        } catch (e) {}

        // plugins: simple non-empty stub
        try {
            Object.defineProperty(proto, 'plugins', {
                get: () => [1, 2, 3, 4, 5],
                configurable: true,
            });
        } catch (e) {}
    } catch (e) {}
})();

// 1. Chrome Runtime (CDP detection bypass)
if (!window.chrome) {
    window.chrome = {};
}
if (!window.chrome.runtime) {
    window.chrome.runtime = {
        // Many detectors only check for presence + basic callability.
        connect: function() { return { onDisconnect: { addListener: function() {} } }; },
        sendMessage: function() {},
    };
}
window.chrome.csi = function() { return { startE: Date.now(), onloadT: Date.now() + 100 }; };
window.chrome.loadTimes = function() { return { requestTime: Date.now() / 1000, finishDocumentLoadTime: (Date.now() + 500) / 1000 }; };

// 2. Permissions Query (notification permission bypass)
const originalQuery = window.navigator.permissions && window.navigator.permissions.query;
if (originalQuery) {
    window.navigator.permissions.query = (parameters) => (
        parameters.name === 'notifications'
            ? Promise.resolve({ state: Notification.permission })
            : originalQuery(parameters)
    );
}

// 3. Canvas Fingerprint Noise Injection
const originalGetContext = HTMLCanvasElement.prototype.getContext;
HTMLCanvasElement.prototype.getContext = function(type, ...args) {
    const context = originalGetContext.apply(this, [type, ...args]);
    if (type === '2d' || type === 'webgl' || type === 'webgl2') {
        if (context) {
            const originalToDataURL = this.toDataURL;
            this.toDataURL = function(...args) {
                const data = originalToDataURL.apply(this, args);
                // Inject minimal noise (last character randomization)
                return data.replace(/.$/, String.fromCharCode(Math.random() * 10 | 0));
            };
        }
    }
    return context;
};

// 4. WebGL Vendor/Renderer Spoofing (SwiftShader masking)
const getParameter = WebGLRenderingContext.prototype.getParameter;
WebGLRenderingContext.prototype.getParameter = function(parameter) {
    if (parameter === 37445) return 'Intel Inc.';
    if (parameter === 37446) return 'Intel Iris OpenGL Engine';
    return getParameter.apply(this, arguments);
};

if (typeof WebGL2RenderingContext !== 'undefined') {
    const getParameter2 = WebGL2RenderingContext.prototype.getParameter;
    WebGL2RenderingContext.prototype.getParameter = function(parameter) {
        if (parameter === 37445) return 'Intel Inc.';
        if (parameter === 37446) return 'Intel Iris OpenGL Engine';
        return getParameter2.apply(this, arguments);
    };
}

// 5. Playwright/Puppeteer Markers Cleanup
delete window.__playwright;
delete window.__puppeteer;
delete window.__selenium;
delete window.callPhantom;
delete window._phantom;

// 6. User-Agent Data (Client Hints) for Chromium 90+
if (navigator.userAgentData) {
    Object.defineProperty(navigator, 'userAgentData', {
        get: () => ({
            brands: [
                { brand: 'Chromium', version: '131' },
                { brand: 'Google Chrome', version: '131' },
                { brand: 'Not_A Brand', version: '24' }
            ],
            mobile: false,
            platform: 'Windows'
        })
    });
}
"#
        .to_string()
    }

    /// Detect if page contains challenge/captcha (iframe-based detection)
    pub(super) fn detect_challenge(&self, html: &str) -> bool {
        let html_lower = html.to_lowercase();
        html_lower.contains("challenges.cloudflare.com")
            || html_lower.contains("hcaptcha.com")
            || html_lower.contains("recaptcha")
            || html_lower.contains("perimeterx")
            || html_lower.contains("datadome.co")
    }
}
