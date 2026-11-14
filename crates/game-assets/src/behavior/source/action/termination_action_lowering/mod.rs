use std::io;

use openzt2_game_data::behavior::{
    action::termination::BehaviorTerminationAction, action_record::BehaviorAction,
};

use crate::source_document::ordered_source_document_types::OrderedSourceDocumentNode as DataNode;

use super::{invalid_behavior_source_data, source_value_reading::read_boolean_attribute_or_false};

pub(super) fn lower_termination_action_source_node(
    termination_action_source_node: &DataNode,
) -> io::Result<Option<BehaviorAction>> {
    if termination_action_source_node.name != "BFBehKill" {
        return Ok(None);
    }
    if termination_action_source_node
        .attributes
        .iter()
        .any(|attribute| {
            !matches!(
                attribute.name(),
                "killSubject" | "killTarget" | "fadeOutPeriod"
            )
        })
        || termination_action_source_node
            .element_children()
            .next()
            .is_some()
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
    if !termination_action.kill_subject && !termination_action.kill_target {
        return Err(invalid_behavior_source_data(
            "BFBehKill does not identify a subject or target",
        ));
    }
    Ok(Some(BehaviorAction::Termination(termination_action)))
}

/// `BFBehKill` on the fence a raptor smashes (`radiusKillType="fence"`): its
/// clip, then the fence becomes its broken variant, as `ZTBehMorph` makes it
/// for an elephant.
// ponytail: only the target fence breaks; the authored `radius` would also
// break fences beside it.
pub(crate) fn lower_fence_kill_actions(node: &DataNode) -> io::Result<Option<[BehaviorAction; 2]>> {
    if node.name != "BFBehKill" || node.attribute("radiusKillType").map(str::trim) != Some("fence")
    {
        return Ok(None);
    }
    if node.attributes.iter().any(|attribute| {
        !matches!(
            attribute.name(),
            "targetAnim"
                | "killTarget"
                | "radius"
                | "radiusKillType"
                | "fadeOutPeriod"
                | "killUsingDestroyEntity"
        )
    }) || !read_boolean_attribute_or_false(node, "killTarget")?
    {
        return Err(invalid_behavior_source_data(
            "BFBehKill on a fence has unmapped authored policy",
        ));
    }
    let animation_clip_asset_key = node
        .attribute("targetAnim")
        .map(str::trim)
        .filter(|key| !key.is_empty())
        .ok_or_else(|| invalid_behavior_source_data("BFBehKill on a fence has no targetAnim"))?
        .to_owned();
    Ok(Some([
        BehaviorAction::AnimationClip(
            openzt2_game_data::behavior::action::animation::BehaviorAnimationClipAction {
                animation_clip_asset_key,
                looping: false,
                physical_object: None,
                play_duration_seconds: None,
                interrupts_current_animation: false,
                interruptible: false,
                ground_fit: None,
                blend_duration_seconds: None,
                playback_speed: None,
            },
        ),
        BehaviorAction::MorphTarget(openzt2_game_data::AssetId::from_key("s_broken")),
    ]))
}
