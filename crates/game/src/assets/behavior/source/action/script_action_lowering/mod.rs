use super::invalid_behavior_source_data;
use crate::assets::source_document::ordered_source_document_types::OrderedSourceDocumentNode;
use openzt2_game_data::{behavior::action_record::BehaviorAction, AssetId};
use std::io;

pub(super) fn lower_script_action(
    node: &OrderedSourceDocumentNode,
) -> io::Result<Option<BehaviorAction>> {
    if node.name != "BFBehScript" {
        return Ok(None);
    }
    if node
        .attributes
        .iter()
        .any(|attribute| !matches!(attribute.name(), "context" | "file" | "function" | "params"))
        || node.element_children().next().is_some()
    {
        return Err(invalid_behavior_source_data(
            "BFBehScript has unmapped entity context or policy",
        ));
    }
    let required = |name| {
        node.attribute(name)
            .filter(|value| !value.trim().is_empty())
            .ok_or_else(|| invalid_behavior_source_data(format!("BFBehScript has no {name}")))
    };
    Ok(Some(BehaviorAction::Script {
        context: AssetId::from_key(&required("context")?.trim().to_ascii_lowercase()),
        file: required("file")?.trim().to_owned(),
        function: required("function")?.trim().to_owned(),
        parameters: node
            .attribute("params")
            .filter(|value| !value.is_empty())
            .map_or_else(Vec::new, |value| {
                value.split(':').map(str::to_owned).collect()
            }),
    }))
}
