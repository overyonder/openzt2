use super::*;
use openzt2_game_data::ui_document::{
    node::{UiNodeDefinition, UiNodeKind},
    widget::UiWidgetRecord,
};

fn node(name: &str, parent: u32, key: &str) -> UiNodeDefinition {
    UiNodeDefinition {
        id: AssetId::from_key(key),
        name: name.into(),
        kind: UiNodeKind::Container,
        parent,
        children: Vec::new(),
        rect: [0.0; 4],
        anchors: [0.0; 4],
        region: default(),
        text: String::new(),
        cursor: default(),
        text_format: default(),
        help: default(),
        cacheable_from_lua: false,
        field_definitions: Vec::new(),
        finance_categories: Vec::new(),
        style: default(),
        bindings: Vec::new(),
        actions: Vec::new(),
        animation: None,
        visuals: Vec::new(),
        hotkeys: Vec::new(),
        widget: UiWidgetRecord::Plain,
        scenario_objective_visual: None,
        flags: default(),
        z: 0,
        tab_order: 0,
    }
}

#[test]
fn child_lookup_scopes_duplicate_text_names_to_the_requested_surface() {
    let document = UiDocument {
        id: default(),
        role: UiDocumentRole::ScenarioWin,
        logical_size: [1024.0, 768.0],
        nodes: vec![
            node("root", 0, "root"),
            node("Scenario Win  Layout", 0, "win"),
            node("inner panel", 1, "win-inner"),
            node("challenge text", 2, "win-text"),
            node("Scenario Failed Layout", 0, "failed"),
            node("challenge text", 4, "failed-text"),
        ],
        gameplay_interaction_cursors: default(),
        construction_placement_preview: default(),
        image_selection_groups: Vec::new(),
        dependencies: Vec::new(),
    };
    assert_eq!(resolve_node(&document, &["challenge text".into()]), None);
    assert_eq!(
        resolve_node(
            &document,
            &["Scenario Win  Layout".into(), "challenge text".into()]
        ),
        Some(AssetId::from_key("win-text"))
    );
    assert_eq!(
        resolve_node(
            &document,
            &["Scenario Failed Layout".into(), "challenge text".into()]
        ),
        Some(AssetId::from_key("failed-text"))
    );
    assert_eq!(
        resolve_node(
            &document,
            &["Scenario Win  Layout".into(), "missing".into()]
        ),
        None
    );
    let mut goals = document;
    goals.role = UiDocumentRole::Scenario;
    goals.nodes = vec![node("goalpanel", 0, "goal-list")];
    let path = vec!["goal panel".into()];
    assert_eq!(role_for(&path), Some(UiDocumentRole::Scenario));
    assert_eq!(
        resolve_node(&goals, &path),
        Some(AssetId::from_key("goal-list"))
    );
}
