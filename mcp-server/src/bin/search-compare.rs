use anyhow::{anyhow, Result};
use shadowcrawl::types::{SearchRequest, SearchResponse, SearchResult};
use std::collections::HashSet;
use std::time::{Duration, Instant};

#[derive(Clone)]
struct Target {
    name: String,
    base_url: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    let native_url =
        std::env::var("NATIVE_HTTP_URL").unwrap_or_else(|_| "http://127.0.0.1:5000".to_string());
    let legacy_url =
        std::env::var("LEGACY_HTTP_URL").unwrap_or_else(|_| "http://127.0.0.1:5001".to_string());

    let targets = vec![
        Target {
            name: "native".to_string(),
            base_url: native_url,
        },
        Target {
            name: "legacy".to_string(),
            base_url: legacy_url,
        },
    ];

    let scenarios = vec![
        ("Technical Query", "Latest Rust 1.76 features summary"),
        ("Current Event", "Stock market news today"),
        (
            "Hard-to-Scrape Site",
            "Latest jobs on LinkedIn for Rust Developer",
        ),
    ];

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(20))
        .build()?;

    println!("\n=== ShadowCrawl Side-by-Side Search Compare ===");
    println!("native: {}", targets[0].base_url);
    println!("legacy: {}", targets[1].base_url);

    for (label, query) in scenarios {
        println!("\n--- Scenario: {} ---", label);
        println!("Query: {}", query);

        for t in &targets {
            let cold = timed_search(&client, t, query).await;
            let warm = timed_search(&client, t, query).await;

            match (cold, warm) {
                (Ok((cold_dur, cold_resp)), Ok((warm_dur, warm_resp))) => {
                    let metrics = analyze_results(&warm_resp.results);
                    println!(
                        "{}: cold={:.2}s warm={:.2}s results={} dup={} date_prefix_snippets={} top3_authority={} top_answer_present={}",
                        t.name,
                        cold_dur.as_secs_f64(),
                        warm_dur.as_secs_f64(),
                        metrics.total,
                        metrics.duplicates,
                        metrics.date_prefix_snippets,
                        metrics.top3_authority,
                        metrics.top_answer_present
                    );

                    if let Some(r0) = warm_resp.results.get(0) {
                        println!("  #1: {}", truncate(&r0.title, 90));
                        println!("      {}", r0.url);
                        println!("      snippet: {}", truncate(&r0.content, 140));
                        if let Some(p) = &r0.published_at {
                            println!("      published_at: {}", p);
                        }
                        if !r0.breadcrumbs.is_empty() {
                            println!("      breadcrumbs: {}", r0.breadcrumbs.join(" > "));
                        }
                        if let Some(ans) = &r0.top_answer {
                            println!("      top_answer: {}", truncate(ans, 160));
                        }
                    }

                    // Keep a one-line hint when cold vs warm diverge strongly.
                    if cold_resp.results.len() != warm_resp.results.len() {
                        println!(
                            "  note: result_count changed cold={} warm={}",
                            cold_resp.results.len(),
                            warm_resp.results.len()
                        );
                    }
                }
                (Err(e), _) | (_, Err(e)) => {
                    println!("{}: ERROR: {}", t.name, e);
                }
            }
        }
    }

    Ok(())
}

async fn timed_search(
    client: &reqwest::Client,
    target: &Target,
    query: &str,
) -> Result<(Duration, SearchResponse)> {
    let url = format!("{}/search", target.base_url.trim_end_matches('/'));
    let body = SearchRequest {
        query: query.to_string(),
    };

    let start = Instant::now();
    let resp = client
        .post(url)
        .json(&body)
        .send()
        .await
        .map_err(|e| anyhow!("{}: request failed: {}", target.name, e))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        return Err(anyhow!(
            "{}: HTTP {}: {}",
            target.name,
            status,
            truncate(&text, 300)
        ));
    }

    let parsed = resp
        .json::<SearchResponse>()
        .await
        .map_err(|e| anyhow!("{}: invalid JSON: {}", target.name, e))?;

    Ok((start.elapsed(), parsed))
}

struct Metrics {
    total: usize,
    duplicates: usize,
    date_prefix_snippets: usize,
    top3_authority: usize,
    top_answer_present: bool,
}

fn analyze_results(results: &[SearchResult]) -> Metrics {
    let mut seen = HashSet::new();
    let mut dup = 0;
    let mut date_prefix = 0;

    for r in results {
        let key = normalize_url_key(&r.url);
        if !seen.insert(key) {
            dup += 1;
        }

        if starts_with_date_prefix(&r.content) {
            date_prefix += 1;
        }
    }

    let top3_authority = results
        .iter()
        .take(3)
        .filter(|r| is_authority_domain(r.domain.as_deref().unwrap_or("")))
        .count();

    let top_answer_present = results
        .iter()
        .take(1)
        .any(|r| r.top_answer.as_deref().unwrap_or("").trim().len() > 30);

    Metrics {
        total: results.len(),
        duplicates: dup,
        date_prefix_snippets: date_prefix,
        top3_authority,
        top_answer_present,
    }
}

fn starts_with_date_prefix(s: &str) -> bool {
    let t = s.trim_start();
    if t.len() < 8 {
        return false;
    }

    // ISO
    if t.get(0..10)
        .map(|p| p.chars().nth(4) == Some('-') && p.chars().nth(7) == Some('-'))
        .unwrap_or(false)
    {
        return true;
    }

    // Month name
    let months = [
        "jan", "feb", "mar", "apr", "may", "jun", "jul", "aug", "sep", "oct", "nov", "dec",
    ];
    let lower = t.to_ascii_lowercase();
    months.iter().any(|m| lower.starts_with(m)) && lower.contains(", 20")
}

fn is_authority_domain(domain: &str) -> bool {
    let d = domain.to_ascii_lowercase();
    if d.is_empty() {
        return false;
    }
    d.ends_with(".gov")
        || d.ends_with(".edu")
        || d == "ietf.org"
        || d.ends_with(".ietf.org")
        || d == "w3.org"
        || d.ends_with(".w3.org")
        || d.contains("docs.rs")
        || d.contains("rust-lang.org")
        || d.contains("learn.microsoft.com")
        || d.contains("github.com")
        || d.contains("stackoverflow.com")
}

fn normalize_url_key(url: &str) -> String {
    let trimmed = url.trim();
    let Ok(mut parsed) = url::Url::parse(trimmed) else {
        return trimmed.to_string();
    };

    parsed.set_fragment(None);

    if parsed.query().is_some() {
        let mut kept: Vec<(String, String)> = Vec::new();
        for (k, v) in parsed.query_pairs() {
            let k_lower = k.to_ascii_lowercase();
            if k_lower.starts_with("utm_")
                || matches!(
                    k_lower.as_str(),
                    "gclid" | "fbclid" | "yclid" | "mc_cid" | "mc_eid" | "ref" | "ref_src"
                )
            {
                continue;
            }
            kept.push((k.to_string(), v.to_string()));
        }
        kept.sort();
        parsed.set_query(None);
        {
            let mut qp = parsed.query_pairs_mut();
            for (k, v) in kept {
                qp.append_pair(&k, &v);
            }
        }
    }

    parsed.to_string()
}

fn truncate(s: &str, n: usize) -> String {
    let s = s.replace('\n', " ").replace('\r', " ");
    if s.chars().count() <= n {
        return s;
    }
    let out: String = s.chars().take(n).collect();
    format!("{}…", out)
}
