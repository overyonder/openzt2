use std::{io, time::Duration};

use openzt2_game_data::behavior::{
    action::wander::{
        BehaviorGroundLocomotionSpeed, BehaviorWanderAction, BehaviorWanderAvoidedEntities,
    },
    action_record::BehaviorAction,
};

use crate::source_document::ordered_source_document_types::OrderedSourceDocumentNode;

use super::{
    invalid_behavior_source_data,
    source_value_reading::{read_boolean_attribute_or_false, read_optional_float_attribute},
};

pub(super) fn lower_wander_action(
    node: &OrderedSourceDocumentNode,
) -> io::Result<Option<BehaviorAction>> {
    if node.name != "BFBehWander" {
        return Ok(None);
    }
    if node.attributes.iter().any(|attribute| {
        !matches!(
            attribute.name(),
            "playTime"
                | "minPlayTime"
                | "maxPlayTime"
                | "locoSpeed"
                | "avoidWater"
                | "avoidEntityRadius"
        )
    }) || node
        .element_children()
        .any(|child| child.name != "avoidEntityTypes")
    {
        return Err(invalid_behavior_source_data(
            "BFBehWander has unmapped policy",
        ));
    }
    let locomotion_speed = match node.attribute("locoSpeed") {
        None | Some("slow") => BehaviorGroundLocomotionSpeed::Slow,
        Some("medium") => BehaviorGroundLocomotionSpeed::Medium,
        Some("fast") => BehaviorGroundLocomotionSpeed::Fast,
        _ => {
            return Err(invalid_behavior_source_data(
                "BFBehWander has unsupported locomotion speed",
            ))
        }
    };
    let fixed = read_optional_float_attribute(node, "playTime")?;
    let mut type_lists = node.element_children();
    let type_list = type_lists.next();
    if type_lists.next().is_some() {
        return Err(invalid_behavior_source_data(
            "duplicate wander avoidance types",
        ));
    }
    let radius = read_optional_float_attribute(node, "avoidEntityRadius")?;
    let avoided_entities = match (radius, type_list) {
        (None, None) => None,
        (Some(radius_m), Some(types)) if radius_m.is_finite() && radius_m >= 0.0 => {
            if !types.attributes.is_empty()
                || types.element_children().any(|entry| {
                    !entry.attributes.is_empty() || entry.element_children().next().is_some()
                })
            {
                return Err(invalid_behavior_source_data(
                    "unsupported wander avoidance type policy",
                ));
            }
            Some(BehaviorWanderAvoidedEntities {
                radius_m,
                types: types
                    .element_children()
                    .map(|entry| openzt2_game_data::AssetId::from_key(&entry.name))
                    .collect(),
            })
        }
        _ => {
            return Err(invalid_behavior_source_data(
                "wander avoidance requires a radius and types",
            ))
        }
    };
    let minimum = read_optional_float_attribute(node, "minPlayTime")?;
    let maximum = read_optional_float_attribute(node, "maxPlayTime")?;
    let (minimum, maximum) = match (fixed, minimum, maximum) {
        (Some(duration), None, None) => (duration, duration),
        (None, Some(minimum), Some(maximum)) => (minimum, maximum),
        _ => {
            return Err(invalid_behavior_source_data(
                "BFBehWander requires one duration policy",
            ))
        }
    };
    let duration = |seconds: f32| {
        Duration::try_from_secs_f64(f64::from(seconds)).map_err(|_| {
            invalid_behavior_source_data("wander duration must be finite and nonnegative")
        })
    };
    let minimum_duration = duration(minimum)?;
    let maximum_duration = duration(maximum)?;
    if maximum_duration < minimum_duration {
        return Err(invalid_behavior_source_data(
            "wander maximum is below minimum",
        ));
    }
    Ok(Some(BehaviorAction::Wander(BehaviorWanderAction {
        minimum_duration,
        maximum_duration,
        locomotion_speed,
        avoid_water: read_boolean_attribute_or_false(node, "avoidWater")?,
        avoided_entities,
    })))
}
