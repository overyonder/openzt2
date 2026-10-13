use std::io;

use openzt2_game_data::behavior::{
    action::head_look::BehaviorHeadLookAction, action_record::BehaviorAction,
};

use crate::source_document::ordered_source_document_types::OrderedSourceDocumentNode as DataNode;

use super::{
    invalid_behavior_source_data,
    source_value_reading::{
        read_boolean_attribute_or_false, read_optional_trimmed_string_attribute,
    },
};

pub(super) fn lower_head_look_action(node: &DataNode) -> io::Result<Option<BehaviorAction>> {
    if node.name != "BFBehHeadLook" {
        return Ok(None);
    }
    if node.element_children().next().is_some()
        || node
            .attributes
            .iter()
            .any(|attribute| !["disengage", "targetNode"].contains(&attribute.name()))
    {
        return Err(invalid_behavior_source_data(
            "BFBehHeadLook has unmapped authored policy",
        ));
    }
    Ok(Some(BehaviorAction::HeadLook(BehaviorHeadLookAction {
        disengage: read_boolean_attribute_or_false(node, "disengage")?,
        target_node: read_optional_trimmed_string_attribute(node, "targetNode"),
    })))
}
