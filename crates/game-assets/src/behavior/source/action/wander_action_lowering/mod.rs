use std::{io, time::Duration};

use openzt2_game_data::behavior::{
    action::wander::{
        BehaviorEvasionAction, BehaviorGroundLocomotionSpeed, BehaviorWanderAction,
        BehaviorWanderAvoidedEntities,
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
                | "avoidLand"
                | "minDepth"
                | "maxDepth"
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
        Some("clean") => BehaviorGroundLocomotionSpeed::Clean,
        Some("glide") => BehaviorGroundLocomotionSpeed::Glide,
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
    // The native radius defaults to zero, which avoids nothing; listed types
    // without a radius are therefore inert, and a radius without types too.
    let radius_m = read_optional_float_attribute(node, "avoidEntityRadius")?.unwrap_or(0.0);
    if !radius_m.is_finite() || radius_m < 0.0 {
        return Err(invalid_behavior_source_data(
            "wander avoidEntityRadius must be finite and nonnegative",
        ));
    }
    let avoided_entities = match type_list {
        None => None,
        Some(types) => {
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
    };
    let depth = |name| -> io::Result<f32> {
        let depth = read_optional_float_attribute(node, name)?.unwrap_or(0.0);
        if !depth.is_finite() || depth < 0.0 {
            return Err(invalid_behavior_source_data(format!(
                "BFBehWander {name} must be finite and nonnegative"
            )));
        }
        Ok(depth)
    };
    let (minimum_depth_m, maximum_depth_m) = (depth("minDepth")?, depth("maxDepth")?);
    if maximum_depth_m < minimum_depth_m {
        return Err(invalid_behavior_source_data(
            "BFBehWander maxDepth is below minDepth",
        ));
    }
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
        avoid_land: read_boolean_attribute_or_false(node, "avoidLand")?,
        minimum_depth_m,
        maximum_depth_m,
    })))
}

const DEFAULT_EVASION_RADIUS_METRES: f32 = 12.0;
const DEFAULT_EVASION_TIME_SECONDS: f32 = 180.0;

/// `BFBehEvasion` with its authored steering radii and time limit.
pub(super) fn lower_evasion_action(
    node: &OrderedSourceDocumentNode,
) -> io::Result<Option<BehaviorAction>> {
    if node.name != "BFBehEvasion" {
        return Ok(None);
    }
    if node.attributes.iter().any(|attribute| {
        !matches!(
            attribute.name(),
            "evasionRadius"
                | "evasionTime"
                | "failOnTimeout"
                | "avoidWater"
                | "avoidLand"
                | "avoidObstacleRadius"
                | "locoSpeed"
        )
    }) || node
        .element_children()
        .any(|child| child.name != "avoidEntityTypes")
    {
        return Err(invalid_behavior_source_data(
            "BFBehEvasion has unmapped policy",
        ));
    }
    let mut type_lists = node.element_children();
    let types = type_lists.next();
    if type_lists.next().is_some()
        || types.is_some_and(|types| {
            !types.attributes.is_empty()
                || types.element_children().any(|entry| {
                    !entry.attributes.is_empty() || entry.element_children().next().is_some()
                })
        })
    {
        return Err(invalid_behavior_source_data(
            "unsupported evasion avoidance type policy",
        ));
    }
    let locomotion_speed = match node.attribute("locoSpeed") {
        None | Some("slow") => BehaviorGroundLocomotionSpeed::Slow,
        Some("medium") => BehaviorGroundLocomotionSpeed::Medium,
        Some("fast") => BehaviorGroundLocomotionSpeed::Fast,
        _ => {
            return Err(invalid_behavior_source_data(
                "BFBehEvasion has unsupported locomotion speed",
            ))
        }
    };
    let radius_m = read_optional_float_attribute(node, "evasionRadius")?
        .unwrap_or(DEFAULT_EVASION_RADIUS_METRES);
    if !radius_m.is_finite() || radius_m < 0.0 {
        return Err(invalid_behavior_source_data(
            "BFBehEvasion evasionRadius must be finite and nonnegative",
        ));
    }
    let obstacle_radius_m =
        read_optional_float_attribute(node, "avoidObstacleRadius")?.unwrap_or(5.0);
    if !obstacle_radius_m.is_finite() || obstacle_radius_m < 0.0 {
        return Err(invalid_behavior_source_data(
            "BFBehEvasion avoidObstacleRadius must be finite and nonnegative",
        ));
    }
    let maximum_duration = Duration::try_from_secs_f64(f64::from(
        read_optional_float_attribute(node, "evasionTime")?.unwrap_or(DEFAULT_EVASION_TIME_SECONDS),
    ))
    .map_err(|_| {
        invalid_behavior_source_data("BFBehEvasion evasionTime must be finite and nonnegative")
    })?;
    Ok(Some(BehaviorAction::Evasion(BehaviorEvasionAction {
        radius_m,
        obstacle_radius_m,
        maximum_duration,
        fail_on_timeout: read_boolean_attribute_or_false(node, "failOnTimeout")?,
        avoid_water: read_boolean_attribute_or_false(node, "avoidWater")?,
        avoid_land: read_boolean_attribute_or_false(node, "avoidLand")?,
        locomotion_speed,
        avoided_types: types
            .into_iter()
            .flat_map(|types| types.element_children())
            .map(|entry| openzt2_game_data::AssetId::from_key(&entry.name))
            .collect(),
    })))
}
