use crate::AppState;
use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::env;
use std::path::Path;
use std::sync::Arc;

#[derive(Debug, Deserialize, Clone)]
struct ProxySourceEntry {
    url: String,
    proxy_type: String,
}

#[derive(Debug, Serialize)]
struct ProxyItem {
    proxy: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    proxy_type: Option<String>,
}

#[derive(Debug)]
pub struct GrabParams {
    pub limit: Option<usize>,
    pub proxy_type: Option<String>,
    pub random: bool,
    pub store_ip_txt: bool,
    pub clear_ip_txt: bool,
    pub append: bool,
}

#[derive(Debug)]
pub struct ListParams {
    pub limit: Option<usize>,
    pub proxy_type: Option<String>,
    pub random: bool,
    pub show_type: bool,
}

pub async fn grab_proxies(state: &Arc<AppState>, params: GrabParams) -> Result<serde_json::Value> {
    let source_path =
        env::var("PROXY_SOURCE_PATH").unwrap_or_else(|_| "proxy_source.json".to_string());
    let source_contents = tokio::fs::read_to_string(&source_path)
        .await
        .map_err(|e| anyhow!("Failed to read proxy source file {}: {}", source_path, e))?;

    let mut sources: Vec<ProxySourceEntry> = serde_json::from_str(&source_contents)
        .map_err(|e| anyhow!("Failed to parse proxy source JSON: {}", e))?;

    let normalized_filter = params
        .proxy_type
        .as_ref()
        .and_then(|t| normalize_proxy_type(t));
    if let Some(filter_type) = &normalized_filter {
        sources.retain(|s| normalize_proxy_type(&s.proxy_type).as_deref() == Some(filter_type));
    }

    if sources.is_empty() {
        return Ok(serde_json::json!({
            "action": "grab",
            "source_path": source_path,
            "returned": 0,
            "total_fetched": 0,
            "warnings": ["No proxy sources matched the requested type"],
            "proxies": []
        }));
    }

    let mut warnings = Vec::new();
    let mut collected: Vec<ProxyItem> = Vec::new();
    let mut seen = HashSet::new();

    for source in sources.iter_mut() {
        let source_type = match normalize_proxy_type(&source.proxy_type) {
            Some(t) => t,
            None => {
                warnings.push(format!(
                    "Unsupported proxy_type in proxy_source.json: {}",
                    source.proxy_type
                ));
                continue;
            }
        };

        let fetch_url = to_raw_url(&source.url);
        let response = state
            .http_client
            .get(&fetch_url)
            .send()
            .await
            .map_err(|e| anyhow!("Failed to fetch proxy source {}: {}", fetch_url, e))?;

        if !response.status().is_success() {
            warnings.push(format!(
                "Proxy source returned status {}: {}",
                response.status(),
                fetch_url
            ));
            continue;
        }

        let body = response
            .text()
            .await
            .map_err(|e| anyhow!("Failed to read proxy source body {}: {}", fetch_url, e))?;

        for line in parse_proxy_lines(&body) {
            let (proxy, inferred_type) = normalize_proxy_line(&line, &source_type);
            if seen.insert(proxy.clone()) {
                collected.push(ProxyItem {
                    proxy,
                    proxy_type: Some(inferred_type),
                });
            }
        }
    }

    if params.random {
        // Randomization deferred - can use different RNG source
        // For now, just return in collected order
    }

    let total_fetched = collected.len();
    if let Some(limit) = params.limit {
        collected.truncate(limit);
    }

    let ip_list_path = env::var("IP_LIST_PATH").unwrap_or_else(|_| "ip.txt".to_string());
    let mut stored_count = 0usize;
    let mut cleared = false;

    if params.clear_ip_txt {
        write_ip_list(&ip_list_path, "").await?;
        cleared = true;
    }

    if params.store_ip_txt {
        let payload = collected
            .iter()
            .map(|p| p.proxy.as_str())
            .collect::<Vec<_>>()
            .join("\n");
        if params.append && !params.clear_ip_txt {
            append_ip_list(&ip_list_path, &payload).await?;
        } else {
            write_ip_list(&ip_list_path, &payload).await?;
        }
        stored_count = collected.len();
    }

    if params.proxy_type.as_deref() == Some("socks4")
        || params.proxy_type.as_deref() == Some("sock4")
    {
        warnings.push("socks4 proxies are collected but may be skipped by the runtime (unsupported by reqwest)".to_string());
    }

    Ok(serde_json::json!({
        "action": "grab",
        "source_path": source_path,
        "proxy_type": normalized_filter,
        "total_fetched": total_fetched,
        "returned": collected.len(),
        "stored": stored_count,
        "cleared_ip_txt": cleared,
        "append": params.append,
        "warnings": warnings,
        "proxies": collected,
        "ip_list_path": ip_list_path
    }))
}

pub async fn list_proxies(params: ListParams) -> Result<serde_json::Value> {
    let ip_list_path = env::var("IP_LIST_PATH").unwrap_or_else(|_| "ip.txt".to_string());
    let content = tokio::fs::read_to_string(&ip_list_path)
        .await
        .map_err(|e| anyhow!("Failed to read ip.txt {}: {}", ip_list_path, e))?;

    let mut entries: Vec<ProxyItem> = Vec::new();
    let filter_type = params
        .proxy_type
        .as_ref()
        .and_then(|t| normalize_proxy_type(t));

    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let inferred = infer_proxy_type(trimmed);
        if let Some(filter) = &filter_type {
            if inferred.as_deref() != Some(filter.as_str()) {
                continue;
            }
        }

        entries.push(ProxyItem {
            proxy: trimmed.to_string(),
            proxy_type: if params.show_type { inferred } else { None },
        });
    }

    if params.random {
        // Randomization deferred - can use different RNG source
        // For now, just return in parsed order
    }

    let total = entries.len();
    if let Some(limit) = params.limit {
        entries.truncate(limit);
    }

    Ok(serde_json::json!({
        "action": "list",
        "ip_list_path": ip_list_path,
        "total": total,
        "returned": entries.len(),
        "random": params.random,
        "limit": params.limit,
        "proxy_type": filter_type,
        "show_proxy_type": params.show_type,
        "proxies": entries
    }))
}

fn normalize_proxy_type(input: &str) -> Option<String> {
    let value = input.trim().to_lowercase();
    match value.as_str() {
        "http" => Some("http".to_string()),
        "https" => Some("https".to_string()),
        "socks5" | "sock5" => Some("socks5".to_string()),
        "socks4" | "sock4" => Some("socks4".to_string()),
        _ => None,
    }
}

fn parse_proxy_lines(body: &str) -> Vec<String> {
    body.lines()
        .map(|line| line.trim())
        .filter(|line| !line.is_empty() && !line.starts_with('#') && !line.starts_with("//"))
        .map(|line| line.to_string())
        .collect()
}

fn normalize_proxy_line(line: &str, proxy_type: &str) -> (String, String) {
    if line.contains("://") {
        if let Ok(parsed) = url::Url::parse(line) {
            let scheme = parsed.scheme().to_string();
            return (line.to_string(), scheme);
        }
        return (line.to_string(), proxy_type.to_string());
    }

    let scheme = match proxy_type {
        "http" => "http",
        "https" => "https",
        "socks5" => "socks5",
        "socks4" => "socks4",
        _ => "http",
    };

    (format!("{}://{}", scheme, line), proxy_type.to_string())
}

fn to_raw_url(url: &str) -> String {
    if url.contains("github.com") && url.contains("/blob/") {
        let trimmed = url.trim();
        if let Ok(parsed) = url::Url::parse(trimmed) {
            if let Some(host) = parsed.host_str() {
                if host == "github.com" {
                    let mut segments = parsed.path().split('/').filter(|s| !s.is_empty());
                    let owner = segments.next();
                    let repo = segments.next();
                    let blob = segments.next();
                    if owner.is_some() && repo.is_some() && blob == Some("blob") {
                        let branch = segments.next();
                        let path: String = segments.collect::<Vec<_>>().join("/");
                        if let Some(branch) = branch {
                            return format!(
                                "https://raw.githubusercontent.com/{}/{}/{}/{}",
                                owner.unwrap(),
                                repo.unwrap(),
                                branch,
                                path
                            );
                        }
                    }
                }
            }
        }
    }

    url.to_string()
}

fn infer_proxy_type(line: &str) -> Option<String> {
    if line.contains("://") {
        if let Ok(parsed) = url::Url::parse(line) {
            return normalize_proxy_type(parsed.scheme());
        }
        return None;
    }

    let (host, port) = parse_host_port(line)?;
    let _ = host;
    match port {
        443 | 8443 => Some("https".to_string()),
        1080 | 1081 | 1082 | 1085 | 1086 | 1088 | 10800 | 10808 | 10809 | 9050 | 9150 | 4145 => {
            Some("socks5".to_string())
        }
        80 | 8000 | 8008 | 8010 | 8080 | 8081 | 8082 | 8083 | 8084 | 8085 | 8111 | 8118 | 8880
        | 8888 | 8889 | 3128 | 3129 => Some("http".to_string()),
        _ => Some("http".to_string()),
    }
}

fn parse_host_port(value: &str) -> Option<(String, u16)> {
    let (host, port_str) = value.rsplit_once(':')?;
    if host.is_empty() {
        return None;
    }
    let port = port_str.parse::<u16>().ok()?;
    Some((host.to_string(), port))
}

async fn write_ip_list(path: &str, content: &str) -> Result<()> {
    if let Some(parent) = Path::new(path).parent() {
        if !parent.as_os_str().is_empty() {
            tokio::fs::create_dir_all(parent).await.ok();
        }
    }

    tokio::fs::write(path, content)
        .await
        .map_err(|e| anyhow!("Failed to write ip.txt {}: {}", path, e))?;
    Ok(())
}

async fn append_ip_list(path: &str, content: &str) -> Result<()> {
    if content.trim().is_empty() {
        return Ok(());
    }

    let existing = tokio::fs::read_to_string(path).await.unwrap_or_default();
    let mut new_content = existing.trim_end().to_string();
    if !new_content.is_empty() {
        new_content.push('\n');
    }
    new_content.push_str(content);

    write_ip_list(path, &new_content).await
}
