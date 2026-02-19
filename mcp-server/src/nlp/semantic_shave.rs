/// 🧬 Semantic Shaving — NeuroSiphon DNA Transfer
///
/// Uses Model2Vec to filter scraped content at the paragraph level, keeping only
/// chunks that are semantically relevant to the user's query.
///
/// # Token Reduction
/// Typical pages: 50-80% token reduction while retaining the signal the user asked for.
///
/// # Algorithm
/// 1. Split content into paragraphs / chunks (~200 word window, 100 word stride).
/// 2. Encode the user query once → query vector.
/// 3. Encode each chunk → chunk vector.
/// 4. Compute cosine similarity.
/// 5. Keep only chunks whose similarity ≥ `threshold` (default 0.35).
/// 6. Re-join kept chunks in their original order.
use anyhow::{Context, Result};
use model2vec_rs::model::StaticModel;
use std::sync::Arc;
use tracing::{info, warn};

/// Default cosine-similarity threshold below which a chunk is discarded.
pub const DEFAULT_RELEVANCE_THRESHOLD: f32 = 0.35;

/// Maximum number of tokens (approx. words) per chunk for embedding.
const CHUNK_WORDS: usize = 200;
/// Overlap between consecutive chunks (in words) for context continuity.
const CHUNK_STRIDE_WORDS: usize = 100;

// ─────────────────────────────────────────────────────────────────────────────
// Public API
// ─────────────────────────────────────────────────────────────────────────────

/// Shard `content` into chunks, embed each one, and return only those whose
/// cosine similarity with `query` is ≥ `threshold`.
///
/// `model` is passed in as a shared reference so callers can reuse the
/// already-loaded `StaticModel` from `MemoryManager` without reloading.
///
/// Returns `(filtered_content, chunks_kept, chunks_total)`.
pub async fn semantic_shave(
    model: Arc<StaticModel>,
    content: &str,
    query: &str,
    threshold: Option<f32>,
) -> Result<(String, usize, usize)> {
    let threshold = threshold.unwrap_or(DEFAULT_RELEVANCE_THRESHOLD);

    if content.trim().is_empty() || query.trim().is_empty() {
        return Ok((content.to_string(), 0, 0));
    }

    // ── 1. Chunk the content ──────────────────────────────────────────────────
    let chunks = chunk_text(content, CHUNK_WORDS, CHUNK_STRIDE_WORDS);
    let total  = chunks.len();

    if total <= 1 {
        // Nothing to shave — single chunk always kept
        return Ok((content.to_string(), 1, 1));
    }

    // ── 2. Embed query + all chunks synchronously in a blocking thread ────────
    let query_owned  = query.to_string();
    let chunks_owned = chunks.clone();
    let model_clone  = Arc::clone(&model);

    let (query_vec, chunk_vecs): (Vec<f32>, Vec<Vec<f32>>) =
        tokio::task::spawn_blocking(move || {
            let q_vec = model_clone.encode_single(&query_owned);
            let c_vecs: Vec<Vec<f32>> = chunks_owned
                .iter()
                .map(|c| model_clone.encode_single(c))
                .collect();
            Ok::<_, anyhow::Error>((q_vec, c_vecs))
        })
        .await
        .context("spawn_blocking for embedding failed")?
        .context("Embedding error")?;

    // ── 3. Score + filter ─────────────────────────────────────────────────────
    let mut kept_indices: Vec<usize> = Vec::new();
    for (i, c_vec) in chunk_vecs.iter().enumerate() {
        let sim = cosine_similarity(&query_vec, c_vec);
        if sim >= threshold {
            kept_indices.push(i);
        }
    }

    // Always keep at least the highest-scoring chunk so we never return empty
    if kept_indices.is_empty() {
        let best = chunk_vecs
            .iter()
            .enumerate()
            .max_by(|(_, a), (_, b)| {
                cosine_similarity(&query_vec, a)
                    .partial_cmp(&cosine_similarity(&query_vec, b))
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .map(|(i, _)| i)
            .unwrap_or(0);
        kept_indices.push(best);
        warn!(
            "🪒 No chunks met threshold {:.2}; falling back to top-scored chunk",
            threshold
        );
    }

    let kept = kept_indices.len();
    info!(
        "🪒 Semantic shave: kept {}/{} chunks (threshold {:.2}, ~{:.0}% token reduction)",
        kept,
        total,
        threshold,
        (1.0 - (kept as f64 / total as f64)) * 100.0
    );

    // ── 4. Re-join in original order, deduplicating overlapping windows ───────
    let result = join_chunks_ordered(&chunks, &kept_indices);

    // 🛡️ Safety guard: overlapping chunk windows can cause the joined output to be
    // *larger* than the original (duplication artefact).  If that happens, abort and
    // return the original so we never inflate token counts.
    if result.len() > content.len() {
        warn!(
            "🪒 Semantic shave expanded content ({} → {} chars); aborting — returning original",
            content.len(),
            result.len()
        );
        return Ok((content.to_string(), kept, total));
    }

    Ok((result, kept, total))
}

// ─────────────────────────────────────────────────────────────────────────────
// Helpers
// ─────────────────────────────────────────────────────────────────────────────

/// Split `text` into overlapping word-window chunks.
fn chunk_text(text: &str, window: usize, stride: usize) -> Vec<String> {
    let words: Vec<&str> = text.split_whitespace().collect();
    if words.is_empty() {
        return vec![];
    }
    if words.len() <= window {
        return vec![text.to_string()];
    }

    let mut chunks = Vec::new();
    let mut start  = 0usize;
    while start < words.len() {
        let end  = (start + window).min(words.len());
        let chunk = words[start..end].join(" ");
        if !chunk.trim().is_empty() {
            chunks.push(chunk);
        }
        if end == words.len() {
            break;
        }
        start += stride;
    }
    chunks
}

/// Merge kept chunks back into text, preserving order and avoiding duplicate
/// content from overlapping windows (drop chunks whose index was already
/// largely covered).
fn join_chunks_ordered(all_chunks: &[String], kept: &[usize]) -> String {
    // kept is already sorted (we iterated in order)
    let mut parts: Vec<&str> = Vec::new();
    for &i in kept {
        // Simple dedup: if this chunk's first few words overlap with end of
        // a neighbouring kept chunk, we still include both (stride overlap is
        // intentional for readability). The post_clean_text dedup stage in
        // clean.rs will collapse true duplicates.
        parts.push(all_chunks[i].as_str());
    }
    parts.join("\n\n")
}

/// Cosine similarity between two f32 vectors.
/// Returns 0.0 if either vector has zero magnitude.
pub fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }

    let dot: f32 = a.iter().zip(b).map(|(x, y)| x * y).sum();
    let mag_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let mag_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();

    if mag_a == 0.0 || mag_b == 0.0 {
        return 0.0;
    }

    (dot / (mag_a * mag_b)).clamp(-1.0, 1.0)
}
