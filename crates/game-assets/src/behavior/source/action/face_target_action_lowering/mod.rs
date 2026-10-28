use std::io;

use openzt2_game_data::behavior::{
    action::face_target::BehaviorFaceTargetAction, action_record::BehaviorAction,
};

use super::{
    invalid_behavior_source_data,
    source_value_reading::{read_boolean_attribute_or_false, read_optional_float_attribute},
};
use crate::source_document::ordered_source_document_types::OrderedSourceDocumentNode as DataNode;

const DEFAULT_ROTATION_TOLERANCE_DEGREES: f32 = 5.0;

/// `rotError` and `rotOffset` are whole degrees. `allowShuffle` chooses the
/// turning clip; turning is a heading change without a clip of its own.
pub(super) fn lower_face_target_action_source_node(
    source_node: &DataNode,
) -> io::Result<Option<BehaviorAction>> {
    if source_node.name != "BFBehFaceTarget" {
        return Ok(None);
    }
    if source_node
        .attributes
        .iter()
        .any(|attribute| !matches!(attribute.name(), "rotError" | "rotOffset" | "allowShuffle"))
        || source_node.element_children().next().is_some()
    {
        return Err(invalid_behavior_source_data(
            "BFBehFaceTarget has unsupported authored policy",
        ));
    }
    read_boolean_attribute_or_false(source_node, "allowShuffle")?;
    let whole_degrees = |name, default: f32| -> io::Result<f32> {
        let degrees = read_optional_float_attribute(source_node, name)?.unwrap_or(default);
        if !degrees.is_finite() {
            return Err(invalid_behavior_source_data(format!(
                "BFBehFaceTarget has invalid {name}"
            )));
        }
        Ok(degrees.trunc().to_radians())
    };
    Ok(Some(BehaviorAction::FaceTarget(BehaviorFaceTargetAction {
        rotation_tolerance_radians: whole_degrees("rotError", DEFAULT_ROTATION_TOLERANCE_DEGREES)?,
        rotation_offset_radians: whole_degrees("rotOffset", 0.0)?,
    })))
}
