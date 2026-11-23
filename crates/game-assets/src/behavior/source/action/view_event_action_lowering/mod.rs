use super::{
    feedback_action_lowering::lower_feedback_data,
    invalid_behavior_source_data,
    source_value_reading::{read_optional_float_attribute, read_optional_trimmed_string_attribute},
};
use crate::source_document::ordered_source_document_types::OrderedSourceDocumentNode as DataNode;
use openzt2_game_data::{
    behavior::{
        action::view_event::{BehaviorViewEventAction, BehaviorViewEventRadius},
        action_record::BehaviorAction,
    },
    AssetId,
};
use std::io;

pub(super) fn lower_view_event_action(node: &DataNode) -> io::Result<Option<BehaviorAction>> {
    if node.name != "ZTBehViewEvent" {
        return Ok(None);
    }
    if node.attributes.iter().any(|attribute| {
        !["viewKey", "viewRadius", "targetType", "edValue", "edDonate"].contains(&attribute.name())
    }) {
        return Err(invalid_behavior_source_data(
            "ZTBehViewEvent has an unknown attribute",
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
    let property = |key| {
        read_optional_trimmed_string_attribute(node, key)
            .map(|value| AssetId::from_key(&value.to_ascii_lowercase()))
    };
    let metres = read_optional_float_attribute(node, "viewRadius")?.unwrap_or_default();
    if !metres.is_finite() || metres < 0.0 {
        return Err(invalid_behavior_source_data(
            "ZTBehViewEvent has invalid viewRadius",
        ));
    }
    Ok(Some(BehaviorAction::ViewEvent(BehaviorViewEventAction {
        view_key: AssetId::from_key(&key.to_ascii_lowercase()),
        radius: (metres > 0.0).then(|| BehaviorViewEventRadius {
            metres,
            target_type: property("targetType"),
        }),
        education_value_property: property("edValue"),
        education_donation_property: property("edDonate"),
        feedback,
    })))
}
