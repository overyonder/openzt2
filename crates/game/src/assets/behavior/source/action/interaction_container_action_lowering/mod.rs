use std::io;

use openzt2_game_data::behavior::action_record::BehaviorAction;

use crate::assets::source_document::ordered_source_document_types::OrderedSourceDocumentNode as DataNode;

use super::invalid_behavior_source_data;

pub(super) fn lower_interaction_container_action_source_node(
    source_node: &DataNode,
) -> io::Result<Option<BehaviorAction>> {
    if source_node.name != "BFBehEnter" {
        return Ok(None);
    }
    if source_node
        .attributes
        .iter()
        .any(|attribute| !matches!(attribute.name(), "containerName" | "enterBehSet"))
        || source_node.element_children().next().is_some()
    {
        return Err(invalid_behavior_source_data(
            "BFBehEnter has unmapped authored policy",
        ));
    }
    let identifier = |name| {
        source_node
            .attribute(name)
            .filter(|value| !value.trim().is_empty())
            .map(|value| openzt2_game_data::AssetId::from_key(&value.trim().to_ascii_lowercase()))
    };
    Ok(Some(BehaviorAction::EnterInteractionContainer {
        container: identifier("containerName"),
        entrance_behavior_set: identifier("enterBehSet"),
    }))
}
