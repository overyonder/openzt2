use std::io;

use openzt2_game_data::{
    behavior::{action::play_set::BehaviorPlaySetAction, action_record::BehaviorAction},
    AssetId,
};

use crate::assets::source_document::ordered_source_document_types::OrderedSourceDocumentNode as DataNode;

use super::{
    invalid_behavior_source_data, source_value_reading::read_optional_trimmed_string_attribute,
};

pub(super) fn lower_play_set_action_source_node(
    source_node: &DataNode,
) -> io::Result<Option<BehaviorAction>> {
    if source_node.name != "BFBehPlaySet" {
        return Ok(None);
    }
    if source_node
        .attributes
        .iter()
        .any(|attribute| attribute.name() != "behSet")
        || source_node.element_children().next().is_some()
    {
        return Err(invalid_behavior_source_data(
            "BFBehPlaySet has unmapped authored policy",
        ));
    }
    let behavior_set_asset_id = read_optional_trimmed_string_attribute(source_node, "behSet")
        .map(|name| AssetId::from_key(&name.to_ascii_lowercase()))
        .ok_or_else(|| invalid_behavior_source_data("BFBehPlaySet has no behSet"))?;
    Ok(Some(BehaviorAction::PlaySet(BehaviorPlaySetAction {
        behavior_set_asset_id,
    })))
}
