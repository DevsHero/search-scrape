use shadowcrawl::{history, mcp_handlers, AppState};
use std::sync::Arc;

#[tokio::test]
#[ignore]
async fn research_history_vector_search_returns_seeded_entry() {
    let tmp_dir =
        std::env::temp_dir().join(format!("shadowcrawl_lancedb_test_{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&tmp_dir).expect("create temp lancedb dir");

    // Initialize LanceDB + Model2Vec (may download model on first run).
    let memory = history::MemoryManager::new(
        tmp_dir
            .to_str()
            .expect("temp dir path should be valid UTF-8"),
    )
    .await
    .expect("init MemoryManager");

    let entry = history::HistoryEntry {
        id: uuid::Uuid::new_v4().to_string(),
        entry_type: history::EntryType::Scrape,
        query: "Example Domain".to_string(),
        topic: "example domain".to_string(),
        summary: "Example Domain is used for illustrative examples in documents.".to_string(),
        full_result: serde_json::json!({
            "url": "https://example.com",
            "title": "Example Domain",
            "content": "Example Domain. This domain is for use in illustrative examples in documents."
        }),
        timestamp: chrono::Utc::now(),
        domain: Some("example.com".to_string()),
        source_type: Some("smoke_test".to_string()),
    };

    memory
        .store_entry(entry)
        .await
        .expect("store entry into LanceDB");

    let http_client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .connect_timeout(std::time::Duration::from_secs(5))
        .build()
        .expect("build http client");

    let state = AppState::new(http_client).with_memory(Arc::new(memory));

    // Exercise the MCP handler (same logic used by both HTTP and stdio MCP).
    let args = serde_json::json!({
        "query": "example domain",
        "limit": 5,
        "threshold": 0.0,
        "entry_type": "scrape"
    });

    let resp = mcp_handlers::research_history::handle(Arc::new(state), &args)
        .await
        .expect("research_history handler should succeed");

    let call_response = resp.0;
    assert!(!call_response.is_error, "handler returned is_error=true");
    assert!(!call_response.content.is_empty(), "no content in response");

    let parsed: serde_json::Value = serde_json::from_str(&call_response.content[0].text)
        .expect("response content should be JSON");

    let total = parsed
        .get("total_results")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);

    assert!(total >= 1, "expected at least one result, got: {total}");
}
