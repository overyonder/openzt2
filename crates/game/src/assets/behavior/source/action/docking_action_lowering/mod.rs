use std::io;

use openzt2_game_data::behavior::{
    action::docking::{BehaviorDockAction, BehaviorDockLocomotionSpeed},
    action_record::BehaviorAction,
};

use crate::assets::source_document::ordered_source_document_types::OrderedSourceDocumentNode as DataNode;

use super::{
    invalid_behavior_source_data,
    source_value_reading::{
        read_boolean_attribute_or_false, read_optional_trimmed_string_attribute,
    },
};

pub(super) fn lower_docking_action_source_node(
    docking_action_source_node: &DataNode,
) -> io::Result<Option<BehaviorAction>> {
    if docking_action_source_node.name != "BFBehDock" {
        return Ok(None);
    }
    if docking_action_source_node
        .attributes
        .iter()
        .any(|attribute| {
            !matches!(
                attribute.name(),
                "subjectNode" | "targetNode" | "targetAnim" | "locoSpeed" | "redock"
            )
        })
        || docking_action_source_node
            .element_children()
            .next()
            .is_some()
    {
        return Err(invalid_behavior_source_data(
            "BFBehDock has unmapped authored policy",
        ));
    }
    let locomotion_speed =
        read_optional_trimmed_string_attribute(docking_action_source_node, "locoSpeed")
            .map(
                |authored_speed| match authored_speed.to_ascii_lowercase().as_str() {
                    "slow" => Ok(BehaviorDockLocomotionSpeed::Slow),
                    "medium" => Ok(BehaviorDockLocomotionSpeed::Medium),
                    "fast" => Ok(BehaviorDockLocomotionSpeed::Fast),
                    _ => Err(invalid_behavior_source_data(
                        "BFBehDock has invalid locoSpeed",
                    )),
                },
            )
            .transpose()?;
    Ok(Some(BehaviorAction::Dock(BehaviorDockAction {
        subject_node_name: read_optional_trimmed_string_attribute(
            docking_action_source_node,
            "subjectNode",
        ),
        target_node_name: read_optional_trimmed_string_attribute(
            docking_action_source_node,
            "targetNode",
        ),
        target_animation_clip_asset_key: read_optional_trimmed_string_attribute(
            docking_action_source_node,
            "targetAnim",
        ),
        locomotion_speed,
        redock: read_boolean_attribute_or_false(docking_action_source_node, "redock")?,
    })))
}
