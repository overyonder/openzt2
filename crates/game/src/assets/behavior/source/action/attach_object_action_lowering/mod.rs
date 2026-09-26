use super::{
    invalid_behavior_source_data, source_value_reading::read_optional_trimmed_string_attribute,
};
use crate::assets::source_document::ordered_source_document_types::OrderedSourceDocumentNode;
use openzt2_game_data::{
    behavior::{action::attach_object::BehaviorAttachObjectAction, action_record::BehaviorAction},
    AssetId,
};
use std::io;

pub(super) fn lower_attach_object_action(
    node: &OrderedSourceDocumentNode,
) -> io::Result<Option<BehaviorAction>> {
    if node.name != "BFBehAttachObject" {
        return Ok(None);
    }
    // Extra attachment policy has no runtime support; ignoring it changes the action.
    if node.attributes.iter().any(|attribute| {
        !["attachEntity", "targetAnim", "container", "detachAction"].contains(&attribute.name())
    }) || node.element_children().next().is_some()
    {
        return Err(invalid_behavior_source_data(
            "BFBehAttachObject has unsupported attachment policy",
        ));
    }
    let entity = read_optional_trimmed_string_attribute(node, "attachEntity")
        .ok_or_else(|| invalid_behavior_source_data("BFBehAttachObject requires attachEntity"))?;
    let identifier = |name| {
        read_optional_trimmed_string_attribute(node, name)
            .map(|value| AssetId::from_key(&value.to_ascii_lowercase()))
            .unwrap_or_default()
    };
    Ok(Some(BehaviorAction::AttachObject(
        BehaviorAttachObjectAction {
            entity: AssetId::from_key(&entity.to_ascii_lowercase()),
            animation: read_optional_trimmed_string_attribute(node, "targetAnim"),
            container: identifier("container"),
            detach_rule: identifier("detachAction"),
        },
    )))
}
