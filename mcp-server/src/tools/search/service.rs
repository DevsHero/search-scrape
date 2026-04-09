use crate::types::SearchResult;
use crate::AppState;
use anyhow::Result;
use async_trait::async_trait;
use std::sync::Arc;

use super::{SearchExtras, SearchParamOverrides};

pub struct SearchExecutionOutcome {
    pub results: Vec<SearchResult>,
    pub extras: SearchExtras,
}

#[async_trait]
pub trait SearchService: Send + Sync {
    async fn search(
        &self,
        state: &Arc<AppState>,
        query: &str,
        overrides: Option<SearchParamOverrides>,
    ) -> Result<SearchExecutionOutcome>;
}
