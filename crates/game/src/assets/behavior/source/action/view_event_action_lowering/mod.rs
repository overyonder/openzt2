use super::{
    feedback_action_lowering::lower_feedback_data,
    invalid_behavior_source_data,
    source_value_reading::{read_optional_float_attribute, read_optional_trimmed_string_attribute},
};
use crate::assets::source_document::ordered_source_document_types::OrderedSourceDocumentNode as DataNode;
use openzt2_game_data::{
    behavior::{action::view_event::BehaviorViewEventAction, action_record::BehaviorAction},
    AssetId,
};
use std::io;

pub(super) fn lower_view_event_action(node: &DataNode) -> io::Result<Option<BehaviorAction>> {
    if node.name != "ZTBehViewEvent" {
        return Ok(None);
    }
    // Positive-radius events need the native 3D box recipient query; education
    // attributes additionally need source property resolution. Keep those explicit.
    if node
        .attributes
        .iter()
        .any(|attribute| !["viewKey", "viewRadius", "targetType"].contains(&attribute.name()))
        || read_optional_float_attribute(node, "viewRadius")?.unwrap_or(0.0) != 0.0
    {
        return Err(invalid_behavior_source_data(
            "ZTBehViewEvent recipient or education policy is not implemented",
        ));
    }
    let key = read_optional_trimmed_string_attribute(node, "viewKey")
        .ok_or_else(|| invalid_behavior_source_data("ZTBehViewEvent has no viewKey"))?;
    let mut children = node.element_children();
    let feedback = children.next().map(lower_feedback_data).transpose()?;
    if children.next().is_some() {
        return Err(invalid_behavior_source_data(
            "ZTBehViewEvent has extra children",
        ));
    }
    // targetType only filters the positive-radius branch in the original.
    Ok(Some(BehaviorAction::ViewEvent(BehaviorViewEventAction {
        view_key: AssetId::from_key(&key.to_ascii_lowercase()),
        feedback,
    })))
}
