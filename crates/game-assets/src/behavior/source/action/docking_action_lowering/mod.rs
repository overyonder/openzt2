use std::io;

use openzt2_game_data::behavior::{
    action::docking::{BehaviorDockAction, BehaviorDockLocomotionSpeed},
    action_record::BehaviorAction,
};

use crate::source_document::ordered_source_document_types::OrderedSourceDocumentNode as DataNode;

use super::{
    invalid_behavior_source_data,
    source_value_reading::{
        read_boolean_attribute_or_false, read_optional_float_attribute,
        read_optional_trimmed_string_attribute,
    },
};

pub(super) fn lower_water_docking_action_source_node(
    docking_action_source_node: &DataNode,
) -> io::Result<Option<BehaviorAction>> {
    if docking_action_source_node.name == "ZTBehDockWater" {
        if docking_action_source_node
            .attributes
            .iter()
            .any(|attribute| !matches!(attribute.name(), "targetAnim" | "subjectNode"))
            || docking_action_source_node
                .element_children()
                .next()
                .is_some()
        {
            return Err(invalid_behavior_source_data(
                "ZTBehDockWater has unmapped policy",
            ));
        }
        let required = |name| {
            read_optional_trimmed_string_attribute(docking_action_source_node, name).ok_or_else(
                || invalid_behavior_source_data(format!("ZTBehDockWater requires {name}")),
            )
        };
        return Ok(Some(BehaviorAction::DockWater(
            openzt2_game_data::behavior::action::docking::BehaviorWaterDockAction {
                subject_node_name: required("subjectNode")?,
                target_animation_clip_asset_key: required("targetAnim")?,
            },
        )));
    }
    Ok(None)
}

pub(super) fn lower_docking_action_source_node(
    docking_action_source_node: &DataNode,
) -> io::Result<Option<BehaviorAction>> {
    if let Some(action) = lower_water_docking_action_source_node(docking_action_source_node)? {
        return Ok(Some(action));
    }
    let radial = match docking_action_source_node.name.as_str() {
        "BFBehDock" => false,
        "BFBehDockRadial" => true,
        _ => return Ok(None),
    };
    // Interpolation and target-node alignment remain unsupported for radial docks.
    if docking_action_source_node
        .attributes
        .iter()
        .any(|attribute| {
            if radial {
                !matches!(
                    attribute.name(),
                    "dockRadius"
                        | "targetRadius"
                        | "rotError"
                        | "targetAnim"
                        | "locoSpeed"
                        | "subjectNode"
                )
            } else {
                !matches!(
                    attribute.name(),
                    "subjectNode" | "targetNode" | "targetAnim" | "locoSpeed" | "redock"
                )
            }
        })
        || docking_action_source_node
            .element_children()
            .next()
            .is_some()
    {
        return Err(invalid_behavior_source_data(format!(
            "{} has unmapped authored policy",
            docking_action_source_node.name
        )));
    }
    let radial_distance_m = if radial {
        // Without a radius the actor docks on the target itself, as the
        // guest viewing tasks do on the path they view from.
        let distance = read_optional_float_attribute(docking_action_source_node, "targetRadius")?
            .unwrap_or(0.0);
        if !distance.is_finite() || distance < 0.0 {
            return Err(invalid_behavior_source_data(
                "BFBehDockRadial has no valid targetRadius",
            ));
        }
        Some(distance)
    } else {
        None
    };
    let arrival_radius_m = read_optional_float_attribute(docking_action_source_node, "dockRadius")?;
    if arrival_radius_m.is_some_and(|radius| !radius.is_finite() || radius <= 0.0) {
        return Err(invalid_behavior_source_data("invalid dockRadius"));
    }
    let rotation_tolerance_degrees =
        read_optional_float_attribute(docking_action_source_node, "rotError")?.unwrap_or(0.0);
    if !rotation_tolerance_degrees.is_finite()
        || !(0.0..=180.0).contains(&rotation_tolerance_degrees)
    {
        return Err(invalid_behavior_source_data("invalid rotError"));
    }
    let locomotion_speed =
        read_optional_trimmed_string_attribute(docking_action_source_node, "locoSpeed")
            .map(
                |authored_speed| match authored_speed.to_ascii_lowercase().as_str() {
                    "slow" => Ok(BehaviorDockLocomotionSpeed::Slow),
                    "medium" => Ok(BehaviorDockLocomotionSpeed::Medium),
                    "fast" => Ok(BehaviorDockLocomotionSpeed::Fast),
                    _ => Err(invalid_behavior_source_data(format!(
                        "{} has invalid locoSpeed",
                        docking_action_source_node.name
                    ))),
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
        radial_distance_m,
        arrival_radius_m,
        rotation_tolerance_radians: rotation_tolerance_degrees.to_radians(),
    })))
}
