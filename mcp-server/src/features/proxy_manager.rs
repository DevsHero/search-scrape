// proxy_manager.rs - Dynamic Proxy Manager for Anti-Bot Evasion
// Manages HTTP/SOCKS5 proxy rotation with intelligent selection based on
// latency, success rate, and priority

use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::RwLock;
use tracing::{info, warn};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProxyConfig {
    pub url: String,
    #[serde(rename = "type")]
    pub proxy_type: String,
    pub priority: u8,
    pub latency_ms: u64,
    pub last_success_timestamp: u64,
    pub last_test_timestamp: u64,
    pub failure_count: u32,
    pub enabled: bool,
    #[serde(default)]
    pub provider: String,
    #[serde(default)]
    pub notes: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProxyManagerConfig {
    pub max_failures_before_disable: u32,
    pub high_latency_threshold_ms: u64,
    pub retry_cooldown_seconds: u64,
    pub test_on_startup: bool,
    pub sticky_session_duration: u64,
}

impl Default for ProxyManagerConfig {
    fn default() -> Self {
        Self {
            max_failures_before_disable: 3,
            high_latency_threshold_ms: 3000,
            retry_cooldown_seconds: 300,
            test_on_startup: false,
            sticky_session_duration: 600,
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ProxyRegistry {
    pub proxies: Vec<ProxyConfig>,
    #[serde(default)]
    pub config: ProxyManagerConfig,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProxyStatus {
    pub total_proxies: usize,
    pub enabled_proxies: usize,
    pub current_proxy: Option<String>,
    pub best_proxy: Option<ProxyConfig>,
    pub current_ip: Option<String>,
}

pub struct ProxyManager {
    registry: Arc<RwLock<ProxyRegistry>>,
    current_proxy_url: Arc<RwLock<Option<String>>>,
    last_switch_time: Arc<RwLock<u64>>,
}

impl ProxyManager {
    /// Load proxies from ip.txt (one proxy URL per line).
    ///
    /// This is the production path: we no longer persist/track proxies via proxies.yaml.
    pub async fn new(ip_list_path: &str) -> Result<Self> {
        let ip_list_default_scheme = std::env::var("IP_LIST_DEFAULT_SCHEME")
            .unwrap_or_else(|_| "http".to_string());

        let ip_content = tokio::fs::read_to_string(ip_list_path)
            .await
            .map_err(|e| anyhow!("Failed to read IP list file {}: {}", ip_list_path, e))?;

        let (ip_proxies, skipped_invalid, skipped_unsupported) =
            build_proxies_from_ip_list(&ip_content, &ip_list_default_scheme);

        let mut existing = HashSet::new();
        let mut proxies = Vec::new();
        for proxy in ip_proxies {
            if existing.insert(proxy.url.clone()) {
                proxies.push(proxy);
            }
        }

        info!(
            "Loaded {} proxies from IP list {} (skipped: {} invalid, {} unsupported)",
            proxies.len(),
            ip_list_path,
            skipped_invalid,
            skipped_unsupported
        );

        let registry = ProxyRegistry {
            proxies,
            config: ProxyManagerConfig::default(),
        };

        let manager = Self {
            registry: Arc::new(RwLock::new(registry)),
            current_proxy_url: Arc::new(RwLock::new(None)),
            last_switch_time: Arc::new(RwLock::new(0)),
        };
        
        // Auto-test proxies on startup if configured
        {
            let reg = manager.registry.read().await;
            let test_on_startup = reg.config.test_on_startup;
            drop(reg);
            
            if test_on_startup {
                info!("Auto-testing all proxies on startup...");
                manager.test_all_proxies().await?;
            }
        }
        
        Ok(manager)
    }
    
    /// Get current proxy status and statistics
    pub async fn get_status(&self) -> Result<ProxyStatus> {
        let registry = self.registry.read().await;
        let current_proxy = self.current_proxy_url.read().await.clone();
        
        let total = registry.proxies.len();
        let enabled = registry.proxies.iter().filter(|p| p.enabled).count();
        let best = self.get_best_proxy_internal(&registry).cloned();
        
        Ok(ProxyStatus {
            total_proxies: total,
            enabled_proxies: enabled,
            current_proxy: current_proxy.clone(),
            best_proxy: best,
            current_ip: None, // Will be populated by external IP check if needed
        })
    }

    /// List proxies in registry (optionally enabled only)
    pub async fn list_proxies(&self, enabled_only: bool) -> Vec<ProxyConfig> {
        let registry = self.registry.read().await;
        registry
            .proxies
            .iter()
            .filter(|proxy| !enabled_only || proxy.enabled)
            .cloned()
            .collect()
    }
    
    /// Select best available proxy based on priority, latency, and success rate
    fn get_best_proxy_internal<'a>(&self, registry: &'a ProxyRegistry) -> Option<&'a ProxyConfig> {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        
        let retry_cooldown = registry.config.retry_cooldown_seconds;
        
        registry
            .proxies
            .iter()
            .filter(|p| {
                // Must be enabled
                if !p.enabled {
                    return false;
                }
                
                // Check cooldown period after failures
                if p.failure_count > 0 {
                    let elapsed = now.saturating_sub(p.last_test_timestamp);
                    if elapsed < retry_cooldown {
                        return false;
                    }
                }
                
                true
            })
            .max_by_key(|p| {
                // Score = priority * 1000 - latency - failure_count * 500
                let base_score = (p.priority as i64) * 1000;
                let latency_penalty = p.latency_ms as i64;
                let failure_penalty = (p.failure_count as i64) * 500;
                
                base_score - latency_penalty - failure_penalty
            })
    }
    
    /// Switch to best available proxy
    pub async fn switch_to_best_proxy(&self) -> Result<String> {
        let registry = self.registry.read().await;
        
        let best = self.get_best_proxy_internal(&registry)
            .ok_or_else(|| anyhow!("No available proxies in registry"))?;
        
        let proxy_url = best.url.clone();
        let priority = best.priority;
        let latency_ms = best.latency_ms;
        drop(registry);
        
        // Update current proxy
        *self.current_proxy_url.write().await = Some(proxy_url.clone());
        *self.last_switch_time.write().await = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        
        info!("Switched to proxy: {} (priority: {}, latency: {}ms)", 
              mask_proxy_credentials(&proxy_url), 
              priority, 
              latency_ms);
        
        Ok(proxy_url)
    }
    
    /// Test connection to target URL through proxy
    pub async fn test_proxy_connection(&self, proxy_url: &str, target_url: &str) -> Result<u64> {
        let start = std::time::Instant::now();
        
        // Build HTTP client with proxy
        let proxy = reqwest::Proxy::all(proxy_url)
            .map_err(|e| anyhow!("Invalid proxy URL: {}", e))?;
        
        let client = reqwest::Client::builder()
            .proxy(proxy)
            .timeout(std::time::Duration::from_secs(10))
            .build()
            .map_err(|e| anyhow!("Failed to build proxy client: {}", e))?;
        
        // Test connection
        let response = client.head(target_url).send().await
            .map_err(|e| anyhow!("Proxy connection test failed: {}", e))?;
        
        let latency = start.elapsed().as_millis() as u64;
        
        if !response.status().is_success() && !response.status().is_redirection() {
            return Err(anyhow!("Proxy returned status: {}", response.status()));
        }
        
        info!("Proxy test OK: {} -> {} in {}ms", 
              mask_proxy_credentials(proxy_url), 
              target_url, 
              latency);
        
        Ok(latency)
    }
    
    /// Update proxy metrics after scrape attempt
    pub async fn record_proxy_result(&self, proxy_url: &str, success: bool, latency_ms: Option<u64>) -> Result<()> {
        let mut registry = self.registry.write().await;
        
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        
        let max_failures = registry.config.max_failures_before_disable;
        
        if let Some(proxy) = registry.proxies.iter_mut().find(|p| p.url == proxy_url) {
            proxy.last_test_timestamp = now;
            
            if success {
                proxy.last_success_timestamp = now;
                proxy.failure_count = 0;
                
                if let Some(latency) = latency_ms {
                    // Update latency with exponential moving average
                    if proxy.latency_ms == 0 {
                        proxy.latency_ms = latency;
                    } else {
                        proxy.latency_ms = (proxy.latency_ms * 7 + latency) / 8;
                    }
                }
                
                info!("Proxy success recorded: {}", mask_proxy_credentials(proxy_url));
            } else {
                proxy.failure_count += 1;
                
                if proxy.failure_count >= max_failures {
                    proxy.enabled = false;
                    warn!("Proxy auto-disabled after {} failures: {}", 
                          proxy.failure_count, 
                          mask_proxy_credentials(proxy_url));
                } else {
                    warn!("Proxy failure recorded ({}/{}): {}", 
                          proxy.failure_count, 
                          max_failures,
                          mask_proxy_credentials(proxy_url));
                }
            }
            
            Ok(())
        } else {
            Err(anyhow!("Proxy URL not found in registry: {}", mask_proxy_credentials(proxy_url)))
        }
    }
    
    /// Test all enabled proxies
    async fn test_all_proxies(&self) -> Result<()> {
        let registry = self.registry.read().await;
        let test_url = "https://httpbin.org/ip";
        
        let mut results = Vec::new();
        
        for proxy in registry.proxies.iter().filter(|p| p.enabled) {
            match self.test_proxy_connection(&proxy.url, test_url).await {
                Ok(latency) => {
                    results.push((proxy.url.clone(), true, Some(latency)));
                }
                Err(e) => {
                    warn!("Proxy test failed: {} - {}", mask_proxy_credentials(&proxy.url), e);
                    results.push((proxy.url.clone(), false, None));
                }
            }
        }
        
        drop(registry);
        
        // Update all results
        for (proxy_url, success, latency) in results {
            self.record_proxy_result(&proxy_url, success, latency).await?;
        }
        
        Ok(())
    }
    
    /// Get current active proxy URL (if any)
    pub async fn get_current_proxy(&self) -> Option<String> {
        self.current_proxy_url.read().await.clone()
    }
    
    /// Check if sticky session is still valid
    pub async fn should_use_sticky_proxy(&self) -> bool {
        let registry = self.registry.read().await;
        let last_switch = *self.last_switch_time.read().await;
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        
        let elapsed = now.saturating_sub(last_switch);
        elapsed < registry.config.sticky_session_duration
    }
}

fn build_proxies_from_ip_list(
    content: &str,
    default_scheme: &str,
) -> (Vec<ProxyConfig>, usize, usize) {
    let mut proxies = Vec::new();
    let mut skipped_invalid = 0usize;
    let mut skipped_unsupported = 0usize;
    let scheme = default_scheme.trim().to_lowercase();

    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        if trimmed.contains("://") {
            match url::Url::parse(trimmed) {
                Ok(parsed) => match parsed.scheme() {
                    "http" | "https" => {
                        proxies.push(build_proxy_config(
                            trimmed.to_string(),
                            parsed.scheme(),
                            4,
                            "ip_list",
                            "source=ip_list",
                        ));
                    }
                    "socks5" => {
                        proxies.push(build_proxy_config(
                            trimmed.to_string(),
                            "socks5",
                            3,
                            "ip_list",
                            "source=ip_list",
                        ));
                    }
                    "socks4" => {
                        skipped_unsupported += 1;
                    }
                    _ => {
                        skipped_unsupported += 1;
                    }
                },
                Err(_) => skipped_invalid += 1,
            }
            continue;
        }

        let (host, port) = match parse_host_port(trimmed) {
            Some(value) => value,
            None => {
                skipped_invalid += 1;
                continue;
            }
        };

        let inferred_scheme = if scheme == "auto" {
            detect_proxy_scheme_by_port(port).to_string()
        } else {
            scheme.clone()
        };

        let inferred_scheme = if inferred_scheme.is_empty() {
            "http".to_string()
        } else {
            inferred_scheme
        };

        match inferred_scheme.as_str() {
            "socks5" => {
                proxies.push(build_proxy_config(
                    format!("socks5://{}:{}", host, port),
                    "socks5",
                    3,
                    "ip_list",
                    &format!("source=ip_list;scheme=socks5;port={}", port),
                ));
            }
            "https" => {
                proxies.push(build_proxy_config(
                    format!("https://{}:{}", host, port),
                    "https",
                    4,
                    "ip_list",
                    &format!("source=ip_list;scheme=https;port={}", port),
                ));
            }
            "http" => {
                proxies.push(build_proxy_config(
                    format!("http://{}:{}", host, port),
                    "http",
                    4,
                    "ip_list",
                    &format!("source=ip_list;scheme=http;port={}", port),
                ));
            }
            "socks4" => {
                skipped_unsupported += 1;
            }
            _ => {
                skipped_unsupported += 1;
            }
        }
    }

    (proxies, skipped_invalid, skipped_unsupported)
}

fn build_proxy_config(
    url: String,
    proxy_type: &str,
    priority: u8,
    provider: &str,
    notes: &str,
) -> ProxyConfig {
    ProxyConfig {
        url,
        proxy_type: proxy_type.to_string(),
        priority,
        latency_ms: 0,
        last_success_timestamp: 0,
        last_test_timestamp: 0,
        failure_count: 0,
        enabled: true,
        provider: provider.to_string(),
        notes: notes.to_string(),
    }
}

fn parse_host_port(value: &str) -> Option<(String, u16)> {
    let mut parts = value.rsplitn(2, ':');
    let port_str = parts.next()?;
    let host = parts.next()?;
    if host.is_empty() {
        return None;
    }
    let port = port_str.parse::<u16>().ok()?;
    Some((host.to_string(), port))
}

fn detect_proxy_scheme_by_port(port: u16) -> &'static str {
    match port {
        443 | 8443 => "https",
        1080 | 1081 | 1082 | 1085 | 1086 | 1088 | 10800 | 10808 | 10809 | 9050 | 9150 | 4145 => "socks5",
        80 | 8000 | 8008 | 8010 | 8080 | 8081 | 8082 | 8083 | 8084 | 8085 | 8111 | 8118 | 8880 | 8888 | 8889 | 3128 | 3129 => "http",
        _ => "http",
    }
}

/// Mask proxy credentials for logging
fn mask_proxy_credentials(url: &str) -> String {
    if let Ok(parsed) = url::Url::parse(url) {
        if parsed.username().is_empty() {
            return url.to_string();
        }
        
        // Mask username and password
        format!(
            "{}://{}:***@{}:{}",
            parsed.scheme(),
            parsed.username(),
            parsed.host_str().unwrap_or("unknown"),
            parsed.port().map(|p| p.to_string()).unwrap_or_default()
        )
    } else {
        url.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_mask_credentials() {
        let url = "http://user:password@proxy.example.com:8080";
        let masked = mask_proxy_credentials(url);
        assert!(masked.contains("user:***"));
        assert!(!masked.contains("password"));
    }
    
    #[tokio::test]
    async fn test_proxy_manager_load() {
        // Requires a real ip list file; intentionally skipped.
    }
}
