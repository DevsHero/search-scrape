use crate::rust_scraper::QualityMode;
use crate::types::ErrorResponse;
use axum::http::StatusCode;
use axum::response::Json;
use serde_json::Value;

pub fn parse_quality_mode(
    arguments: &Value,
) -> Result<QualityMode, (StatusCode, Json<ErrorResponse>)> {
    let raw = arguments
        .get("quality_mode")
        .and_then(|v| v.as_str())
        .unwrap_or("balanced");

    match QualityMode::parse_str(raw) {
        Some(mode) => Ok(mode),
        None => Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: "Invalid quality_mode. Allowed values: balanced, aggressive".to_string(),
            }),
        )),
    }
}
