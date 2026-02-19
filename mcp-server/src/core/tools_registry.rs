use crate::mcp::tooling::tool_catalog;
use serde_json::Value;
use std::collections::HashMap;

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
    public_to_internal: HashMap<String, String>,
    internal_to_public: HashMap<String, PublicToolSpec>,
    // Per-internal-tool argument key aliases (public_key -> internal_key)
    arg_aliases: HashMap<String, HashMap<String, String>>,
    // Per-internal-tool enum value aliases: tool -> field -> (public_value -> internal_value)
    enum_aliases: HashMap<String, HashMap<String, HashMap<String, String>>>,
}

impl ToolRegistry {
    pub fn load() -> Self {
        let internal_catalog = tool_catalog();
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

        // Extra tool-name aliases to reduce agent confusion and steer calls toward
        // ShadowCrawl's token-efficient tools instead of IDE-provided fetchers.
        // These map *public* names to stable internal tool names.
        // Backwards compatibility: internal names are always accepted too.
        registry
            .public_to_internal
            .insert("web_fetch".to_string(), "scrape_url".to_string());
        registry
            .public_to_internal
            .insert("fetch_url".to_string(), "scrape_url".to_string());
        registry
            .public_to_internal
            .insert("fetch_webpage".to_string(), "scrape_url".to_string());
        registry
            .public_to_internal
            .insert("webpage_fetch".to_string(), "scrape_url".to_string());
        registry
            .public_to_internal
            .insert("web_fetch_batch".to_string(), "scrape_batch".to_string());
        registry
            .public_to_internal
            .insert("fetch_url_batch".to_string(), "scrape_batch".to_string());

        registry
            .public_to_internal
            .insert("web_crawl".to_string(), "crawl_website".to_string());
        registry
            .public_to_internal
            .insert("site_crawl".to_string(), "crawl_website".to_string());

        registry
            .public_to_internal
            .insert("extract_fields".to_string(), "extract_structured".to_string());
        registry
            .public_to_internal
            .insert("structured_extract".to_string(), "extract_structured".to_string());

        registry
            .public_to_internal
            .insert("memory_search".to_string(), "research_history".to_string());

        registry
            .public_to_internal
            .insert("proxy_control".to_string(), "proxy_manager".to_string());

        registry
            .public_to_internal
            .insert("hitl_web_fetch".to_string(), "non_robot_search".to_string());
        registry
            .public_to_internal
            .insert("human_web_fetch".to_string(), "non_robot_search".to_string());

        for internal in internal_catalog {
            let internal_name = internal.name.to_string();
            let icons = internal.icons.into_iter().map(|s| s.to_string()).collect();

            // Public-facing tool names are designed to be "agent-attractive" verbs.
            // Internal names remain stable for handler routing and for backwards compatibility.
            let public_name = match internal_name.as_str() {
                "search_web" => "web_search".to_string(),
                "search_structured" => "web_search_json".to_string(),
                "scrape_url" => "web_fetch".to_string(),
                "scrape_batch" => "web_fetch_batch".to_string(),
                "crawl_website" => "web_crawl".to_string(),
                "extract_structured" => "extract_fields".to_string(),
                "research_history" => "memory_search".to_string(),
                "proxy_manager" => "proxy_control".to_string(),
                "non_robot_search" => "hitl_web_fetch".to_string(),
                _ => internal_name.clone(),
            };
            let public_title = internal.title.to_string();
            let public_description = internal.description.to_string();

            let public_input_schema =
                registry.sanitize_schema_for_public(&internal_name, internal.input_schema);

            registry.internal_to_public.insert(
                internal_name.clone(),
                PublicToolSpec {
                    public_name,
                    public_title,
                    public_description,
                    public_input_schema,
                    icons,
                },
            );

            // Accept calls using either the public name or the internal name.
            // Example: "non_robot_search" (public) -> "non_robot_search" (internal)
            //          "non_robot_search" (internal) -> "non_robot_search" (internal)
            if let Some(spec) = registry.internal_to_public.get(&internal_name) {
                registry
                    .public_to_internal
                    .insert(spec.public_name.clone(), internal_name.clone());
            }
            registry
                .public_to_internal
                .insert(internal_name.clone(), internal_name.clone());
        }

        registry
    }

    pub fn public_specs(&self) -> Vec<PublicToolSpec> {
        let mut tools: Vec<_> = self.internal_to_public.values().cloned().collect();
        tools.sort_by(|a, b| a.public_name.cmp(&b.public_name));
        tools
    }

    pub fn resolve_incoming_tool_name(&self, incoming: &str) -> Option<String> {
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
