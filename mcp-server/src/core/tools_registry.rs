use crate::mcp::tooling::tool_catalog;
use serde::Deserialize;
use serde_json::Value;
use std::collections::HashMap;
use std::path::PathBuf;
use tracing::{info, warn};

#[derive(Clone, Debug, Deserialize)]
pub struct ToolMetadataEntry {
    pub name: String,
    #[serde(default)]
    pub title: Option<String>,
    pub description: String,
    #[serde(default)]
    pub input_hints: Option<std::collections::HashMap<String, String>>,
}

#[derive(Clone, Debug)]
pub struct PublicToolSpec {
    pub public_name: String,
    pub public_title: String,
    pub public_description: String,
    pub public_input_schema: Value,
    pub icons: Vec<String>,
}

#[derive(Clone, Debug, Default)]
pub struct ToolRegistry {
    internal_to_public: HashMap<String, PublicToolSpec>,
    public_to_internal: HashMap<String, String>,
    // Per-internal-tool argument key aliases (public_key -> internal_key)
    arg_aliases: HashMap<String, HashMap<String, String>>,
    // Per-internal-tool enum value aliases: tool -> field -> (public_value -> internal_value)
    enum_aliases: HashMap<String, HashMap<String, HashMap<String, String>>>,
}

impl ToolRegistry {
    pub fn load() -> Self {
        let internal_catalog = tool_catalog();
        let (metadata_map, source) = load_tools_metadata();
        if let Some(source) = source {
            info!("tool_metadata: loaded from {}", source.display());
        } else {
            warn!("tool_metadata: metadata file not found; using built-in safe defaults");
        }

        let mut registry = ToolRegistry::default();

        // Define schema + argument sanitization rules.
        // Note: the public schema is what clients see, and we map public arguments back to internal.
        registry.arg_aliases.insert(
            "non_robot_search".to_string(),
            HashMap::from([(
                "challenge_grace_seconds".to_string(),
                "captcha_grace_seconds".to_string(),
            )]),
        );

        registry.enum_aliases.insert(
            "research_history".to_string(),
            HashMap::from([(
                "entry_type".to_string(),
                HashMap::from([("sync".to_string(), "scrape".to_string())]),
            )]),
        );

        for internal in internal_catalog {
            let internal_name = internal.name.to_string();
            let icons = internal.icons.into_iter().map(|s| s.to_string()).collect();

            let (public_name, public_title, public_description) =
                match metadata_map.as_ref().and_then(|m| m.get(&internal_name)) {
                    Some(meta) => (
                        meta.name.clone(),
                        meta.title
                            .clone()
                            .unwrap_or_else(|| safe_fallback_title(&internal_name)),
                        meta.description.clone(),
                    ),
                    None => (
                        safe_fallback_public_name(&internal_name),
                        safe_fallback_title(&internal_name),
                        safe_fallback_description(&internal_name),
                    ),
                };

            let public_input_schema =
                registry.sanitize_schema_for_public(&internal_name, internal.input_schema);

            if let Some(existing) = registry
                .public_to_internal
                .insert(public_name.clone(), internal_name.clone())
            {
                warn!(
                    "tool_metadata: public tool name collision: {} already mapped to {}; now also maps to {}",
                    public_name, existing, internal_name
                );
            }

            registry.internal_to_public.insert(
                internal_name,
                PublicToolSpec {
                    public_name,
                    public_title,
                    public_description,
                    public_input_schema,
                    icons,
                },
            );
        }

        registry
    }

    pub fn public_specs(&self) -> Vec<PublicToolSpec> {
        let mut tools: Vec<_> = self.internal_to_public.values().cloned().collect();
        tools.sort_by(|a, b| a.public_name.cmp(&b.public_name));
        tools
    }

    pub fn resolve_incoming_tool_name(&self, incoming: &str) -> Option<String> {
        if self.internal_to_public.contains_key(incoming) {
            return Some(incoming.to_string());
        }
        self.public_to_internal.get(incoming).cloned()
    }

    pub fn map_public_arguments_to_internal(
        &self,
        internal_tool_name: &str,
        public_arguments: Value,
    ) -> Value {
        let mut args = match public_arguments {
            Value::Object(map) => map,
            other => return other,
        };

        if let Some(alias_map) = self.arg_aliases.get(internal_tool_name) {
            for (public_key, internal_key) in alias_map {
                if let Some(v) = args.remove(public_key) {
                    args.insert(internal_key.clone(), v);
                }
            }
        }

        if let Some(field_maps) = self.enum_aliases.get(internal_tool_name) {
            for (field, value_map) in field_maps {
                if let Some(Value::String(s)) = args.get(field).cloned() {
                    if let Some(internal_value) = value_map.get(&s) {
                        args.insert(field.clone(), Value::String(internal_value.clone()));
                    }
                }
            }
        }

        Value::Object(args)
    }

    pub fn public_tool_name_for_internal(&self, internal_tool_name: &str) -> Option<&str> {
        self.internal_to_public
            .get(internal_tool_name)
            .map(|s| s.public_name.as_str())
    }

    pub fn public_description_for_internal(&self, internal_tool_name: &str) -> Option<&str> {
        self.internal_to_public
            .get(internal_tool_name)
            .map(|s| s.public_description.as_str())
    }

    fn sanitize_schema_for_public(&self, internal_tool_name: &str, schema: Value) -> Value {
        let mut schema = schema;

        if let Some(alias_map) = self.arg_aliases.get(internal_tool_name) {
            for (public_key, internal_key) in alias_map {
                schema = rename_schema_property(schema, internal_key, public_key);
            }
        }

        // Present enum values using the public vocabulary.
        if let Some(field_maps) = self.enum_aliases.get(internal_tool_name) {
            for (field, value_map) in field_maps {
                // value_map is public->internal; invert for schema presentation.
                let mut inverted: HashMap<String, String> = HashMap::new();
                for (public_v, internal_v) in value_map {
                    inverted.insert(internal_v.clone(), public_v.clone());
                }
                schema = rewrite_schema_enum_values(schema, field, &inverted);
            }
        }

        schema
    }
}

fn load_tools_metadata() -> (Option<HashMap<String, ToolMetadataEntry>>, Option<PathBuf>) {
    let explicit = std::env::var("SHADOWCRAWL_TOOLS_METADATA_PATH")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .map(PathBuf::from);

    let mut candidates: Vec<PathBuf> = Vec::new();
    if let Some(p) = explicit {
        candidates.push(p);
    }

    // Common system location (useful in container images)
    candidates.push(PathBuf::from("/etc/shadowcrawl/tools_metadata.json"));

    // Try current working directory and a parent directory (common when running from mcp-server/)
    if let Ok(cwd) = std::env::current_dir() {
        candidates.push(cwd.join("tools_metadata.json"));
        candidates.push(cwd.join("../tools_metadata.json"));
        candidates.push(cwd.join("../../tools_metadata.json"));
    }

    for path in candidates {
        if !path.exists() {
            continue;
        }
        match std::fs::read_to_string(&path) {
            Ok(raw) => match serde_json::from_str::<HashMap<String, ToolMetadataEntry>>(&raw) {
                Ok(map) => return (Some(map), Some(path)),
                Err(e) => {
                    warn!(
                        "tool_metadata: failed to parse {} ({}); ignoring and continuing",
                        path.display(),
                        e
                    );
                }
            },
            Err(e) => {
                warn!(
                    "tool_metadata: failed to read {} ({}); ignoring and continuing",
                    path.display(),
                    e
                );
            }
        }
    }

    (None, None)
}

fn safe_fallback_public_name(internal: &str) -> String {
    match internal {
        "search_web" => "web_source_discovery".to_string(),
        "search_structured" => "top_results_synchronizer".to_string(),
        "scrape_url" => "page_content_synchronizer".to_string(),
        "scrape_batch" => "batch_content_sync".to_string(),
        "crawl_website" => "sitemap_structure_analyzer".to_string(),
        "extract_structured" => "structured_data_extractor".to_string(),
        "research_history" => "research_session_index".to_string(),
        "proxy_manager" => "network_context_provider".to_string(),
        "non_robot_search" => "fetch_web_high_fidelity".to_string(),
        other => format!("tool_{}", other),
    }
}

fn safe_fallback_title(internal: &str) -> String {
    match internal {
        "search_web" => "Web Source Discovery".to_string(),
        "search_structured" => "Top Results Synchronizer".to_string(),
        "scrape_url" => "Page Content Synchronizer".to_string(),
        "scrape_batch" => "Batch Content Sync".to_string(),
        "crawl_website" => "Sitemap Structure Analyzer".to_string(),
        "extract_structured" => "Structured Data Extractor".to_string(),
        "research_history" => "Research Session Index".to_string(),
        "proxy_manager" => "Network Context Provider".to_string(),
        "non_robot_search" => "High-Fidelity Web Renderer".to_string(),
        other => other.to_string(),
    }
}

fn safe_fallback_description(internal: &str) -> String {
    match internal {
        "search_web" => "Discovers public web sources using federated queries and relevance ranking for research workflows. Technical reason: improves source coverage and reduces manual source hunting.".to_string(),
        "search_structured" => "Runs a query, then synchronizes the top results into a consistent summary payload for quick review. Technical reason: standardizes result triage across providers and formats.".to_string(),
        "scrape_url" => "Synchronizes a single page into cleaned text or structured JSON with link context when requested. Technical reason: provides consistent downstream inputs for analysis and note-taking.".to_string(),
        "scrape_batch" => "Synchronizes many pages in parallel with concurrency control and consistent output formatting. Technical reason: improves throughput while keeping results comparable.".to_string(),
        "crawl_website" => "Traverses a site link graph within configured bounds to produce a structured view of pages and relationships. Technical reason: supports architecture analysis and content inventory.".to_string(),
        "extract_structured" => "Extracts user-defined fields from a page into a structured JSON object. Technical reason: enables schema-aligned research capture for repeatable evaluation.".to_string(),
        "research_history" => "Queries prior research artifacts by meaning with configurable recall controls. Technical reason: reduces repeated work by reusing previously synchronized materials.".to_string(),
        "proxy_manager" => "Manages regional network endpoints and rotation policies to keep outbound requests stable and observable. Technical reason: improves connectivity consistency for global research.".to_string(),
        "non_robot_search" => "Synchronizes content from advanced web applications using a full rendering engine to preserve DOM integrity and JavaScript execution. Technical reason: supports modern frameworks and dynamic navigation where static retrieval is insufficient.".to_string(),
        other => format!(
            "Provides a specialized capability for research workflows. Technical reason: supports consistent automation for {}.",
            other
        ),
    }
}

fn rename_schema_property(mut schema: Value, from_key: &str, to_key: &str) -> Value {
    let Some(props) = schema.get_mut("properties").and_then(|v| v.as_object_mut()) else {
        return schema;
    };

    if let Some(v) = props.remove(from_key) {
        props.insert(to_key.to_string(), v);
    }

    if let Some(required) = schema.get_mut("required").and_then(|v| v.as_array_mut()) {
        for item in required.iter_mut() {
            if item.as_str() == Some(from_key) {
                *item = Value::String(to_key.to_string());
            }
        }
    }

    schema
}

fn rewrite_schema_enum_values(
    mut schema: Value,
    field: &str,
    mapping: &HashMap<String, String>,
) -> Value {
    let Some(props) = schema.get_mut("properties").and_then(|v| v.as_object_mut()) else {
        return schema;
    };

    let Some(field_schema) = props.get_mut(field) else {
        return schema;
    };

    let Some(enum_values) = field_schema.get_mut("enum").and_then(|v| v.as_array_mut()) else {
        return schema;
    };

    for item in enum_values.iter_mut() {
        if let Some(s) = item.as_str() {
            if let Some(replacement) = mapping.get(s) {
                *item = Value::String(replacement.clone());
            }
        }
    }

    schema
}
