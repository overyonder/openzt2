use std::io;

use openzt2_game_data::{
    behavior::{
        action::{animation::BehaviorAnimationClipAction, termination::BehaviorTerminationAction},
        action_record::BehaviorAction,
    },
    AssetId,
};

use crate::source_document::ordered_source_document_types::OrderedSourceDocumentNode as DataNode;

use super::{invalid_behavior_source_data, source_value_reading::read_boolean_attribute_or_false};

pub(super) fn lower_termination_action_source_node(
    termination_action_source_node: &DataNode,
) -> io::Result<Option<BehaviorAction>> {
    if termination_action_source_node.name != "BFBehKill" {
        return Ok(None);
    }
    let node = termination_action_source_node;
    if node.attributes.iter().any(|attribute| {
        !matches!(
            attribute.name(),
            "killSubject"
                | "killTarget"
                | "fadeOutPeriod"
                | "targetAnim"
                | "killUsingDestroyEntity"
                | "radius"
                | "radiusKillType"
        )
    }) || node.element_children().next().is_some()
    {
        return Err(invalid_behavior_source_data(
            "BFBehKill has unmapped authored policy",
        ));
    }
    let seconds = super::source_value_reading::read_optional_float_attribute(
        termination_action_source_node,
        "fadeOutPeriod",
    )?
    .unwrap_or(0.0);
    let fade_out_period = std::time::Duration::try_from_secs_f32(seconds)
        .map_err(|_| invalid_behavior_source_data("BFBehKill has invalid fadeOutPeriod"))?;
    let termination_action = BehaviorTerminationAction {
        fade_out_period,
        kill_subject: read_boolean_attribute_or_false(
            termination_action_source_node,
            "killSubject",
        )?,
        kill_target: read_boolean_attribute_or_false(termination_action_source_node, "killTarget")?,
    };
    let radius = super::source_value_reading::read_optional_float_attribute(node, "radius")?;
    if radius.is_some_and(|radius| !radius.is_finite() || radius < 0.0) {
        return Err(invalid_behavior_source_data(
            "BFBehKill has an invalid radius",
        ));
    }
    let radius = radius
        .filter(|radius| *radius > 0.0)
        .map(|radius| {
            let kind = super::source_value_reading::read_optional_trimmed_string_attribute(
                node,
                "radiusKillType",
            )
            .ok_or_else(|| invalid_behavior_source_data("BFBehKill has no radiusKillType"))?;
            Ok::<_, io::Error>((radius, AssetId::from_key(&kind.to_ascii_lowercase())))
        })
        .transpose()?;
    if !termination_action.kill_subject && !termination_action.kill_target && radius.is_none() {
        return Err(invalid_behavior_source_data(
            "BFBehKill does not identify a subject or target",
        ));
    }
    let animation =
        super::source_value_reading::read_optional_trimmed_string_attribute(node, "targetAnim")
            .map(effect_clip);
    let destroy = read_boolean_attribute_or_false(node, "killUsingDestroyEntity")?;
    Ok(Some(
        if animation.is_none() && radius.is_none() && !destroy {
            BehaviorAction::Termination(termination_action)
        } else {
            BehaviorAction::AnimationKill {
                animation,
                termination: termination_action,
                radius,
                destroy,
            }
        },
    ))
}

pub(super) fn lower_whap_action_source_node(node: &DataNode) -> io::Result<Option<BehaviorAction>> {
    if node.name != "BFBehWhap" {
        return Ok(None);
    }
    if node.attributes.iter().any(|attribute| {
        !matches!(
            attribute.name(),
            "targetAnim" | "whapAngle" | "randomOffset" | "whapNow" | "interruptFlag" | "groundFit"
        )
    }) || node.element_children().next().is_some()
    {
        return Err(invalid_behavior_source_data(
            "BFBehWhap has unmapped authored policy",
        ));
    }
    let angle = |name| -> io::Result<Option<f32>> {
        let value = super::source_value_reading::read_optional_float_attribute(node, name)?;
        if value.is_some_and(|angle| !angle.is_finite()) {
            return Err(invalid_behavior_source_data(
                "BFBehWhap has an invalid angle",
            ));
        }
        Ok(value)
    };
    let animation =
        super::source_value_reading::read_optional_trimmed_string_attribute(node, "targetAnim")
            .filter(|_| node.attribute("whapNow") != Some("true"))
            .map(effect_clip);
    Ok(Some(BehaviorAction::Whap {
        animation,
        angle_degrees: angle("whapAngle")?,
        random_offset_degrees: angle("randomOffset")?,
    }))
}

fn effect_clip(animation_clip_asset_key: String) -> BehaviorAnimationClipAction {
    BehaviorAnimationClipAction {
        animation_clip_asset_key,
        looping: false,
        physical_object: None,
        play_duration_seconds: None,
        interrupts_current_animation: false,
        interruptible: false,
        ground_fit: None,
        blend_duration_seconds: None,
        playback_speed: None,
    }
}
