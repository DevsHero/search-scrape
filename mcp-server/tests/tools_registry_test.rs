use shadowcrawl::core::tools_registry::ToolRegistry;

#[test]
fn public_tool_names_resolve_to_internal() {
    let registry = ToolRegistry::load();

    let mut seen = std::collections::HashSet::new();
    for spec in registry.public_specs() {
        assert!(
            seen.insert(spec.public_name.clone()),
            "duplicate public tool name"
        );
        let internal = registry
            .resolve_incoming_tool_name(&spec.public_name)
            .expect("public name should resolve to an internal name");
        assert!(
            registry.public_tool_name_for_internal(&internal).is_some(),
            "internal tool should have a public name"
        );
    }
}

#[test]
fn public_schema_is_sanitized_for_known_fields() {
    let registry = ToolRegistry::load();
    let specs = registry.public_specs();

    let renderer = specs
        .iter()
        .find(|s| s.public_name == "non_robot_search")
        .expect("expected renderer tool");

    let props = renderer
        .public_input_schema
        .get("properties")
        .and_then(|v| v.as_object())
        .expect("schema properties should be an object");
    assert!(
        props.contains_key("challenge_grace_seconds"),
        "expected sanitized grace field"
    );

    let history = specs
        .iter()
        .find(|s| s.public_name == "research_session_index")
        .expect("expected history tool");

    let entry_type_enum = history
        .public_input_schema
        .get("properties")
        .and_then(|v| v.get("entry_type"))
        .and_then(|v| v.get("enum"))
        .and_then(|v| v.as_array())
        .expect("entry_type.enum should exist");

    let values: Vec<&str> = entry_type_enum.iter().filter_map(|v| v.as_str()).collect();
    assert!(values.contains(&"sync"), "expected sanitized enum value");
}
