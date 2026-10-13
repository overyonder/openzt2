use super::{
    invalid_behavior_source_data, source_value_reading::read_optional_trimmed_string_attribute,
};
use crate::source_document::ordered_source_document_types::OrderedSourceDocumentNode;
use openzt2_game_data::{behavior::action_record::BehaviorAction, AssetId};
use std::io;

pub(super) fn lower_spawn_object_action(
    node: &OrderedSourceDocumentNode,
) -> io::Result<Option<BehaviorAction>> {
    if node.name != "BFBehSpawn" {
        return Ok(None);
    }
    if node
        .attributes
        .iter()
        .any(|attribute| !["spawnEntity", "subjectNode", "targetAnim"].contains(&attribute.name()))
        || node.element_children().next().is_some()
    {
        return Err(invalid_behavior_source_data(
            "BFBehSpawn has unsupported spawn policy",
        ));
    }
    let required = |name| {
        read_optional_trimmed_string_attribute(node, name)
            .ok_or_else(|| invalid_behavior_source_data(format!("BFBehSpawn requires {name}")))
    };
    Ok(Some(BehaviorAction::SpawnObject {
        entity: AssetId::from_key(&required("spawnEntity")?.to_ascii_lowercase()),
        subject_node: required("subjectNode")?,
        animation: required("targetAnim")?,
    }))
}
