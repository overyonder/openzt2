use std::io;

use openzt2_game_data::behavior::{
    action::animation::{
        BehaviorAnimationClipAction, BehaviorAnimationPlaybackSpeed, BehaviorAnimationSwitchAction,
    },
    action_record::BehaviorAction,
};

use crate::source_document::ordered_source_document_types::OrderedSourceDocumentNode as DataNode;

use super::{
    invalid_behavior_source_data,
    source_value_reading::{read_boolean_attribute_or_false, read_optional_float_attribute},
};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source_document::{
        blue_fang_source_document_parsing::parse_blue_fang_source_document, path::AssetPath,
    };

    #[test]
    fn authored_loop_flag_survives_animation_lowering() {
        let source = parse_blue_fang_source_document(
            AssetPath::new("guest.beh"),
            br#"<behaviors><BFBehAnimate targetAnim="Stand_Idle" loopFlag="true"/></behaviors>"#,
        )
        .expect("source");
        let action = lower_animation_action_source_node(
            source.root.element_children().next().expect("action"),
        )
        .expect("lower")
        .expect("animation");
        let BehaviorAction::AnimationClip(clip) = action else {
            panic!("animation")
        };
        assert!(clip.looping);
        assert_eq!(clip.animation_clip_asset_key, "Stand_Idle");
    }
}

pub(super) fn lower_animation_action_source_node(
    animation_action_source_node: &DataNode,
) -> io::Result<Option<BehaviorAction>> {
    if animation_action_source_node.name == "BFBehAnimateSwitch" {
        return lower_animation_switch(animation_action_source_node).map(Some);
    }
    if animation_action_source_node.name != "BFBehAnimate" {
        return Ok(None);
    }
    if animation_action_source_node
        .attributes
        .iter()
        .any(|attribute| {
            !matches!(
                attribute.name(),
                "targetAnim"
                    | "loopFlag"
                    | "physObj"
                    | "playTime"
                    | "playtime"
                    | "interruptFlag"
                    | "interruptible"
                    | "groundFit"
                    | "blendDuration"
                    | "animSpeed"
                    | "minAnimSpeed"
                    | "maxAnimSpeed"
            )
        })
        || animation_action_source_node
            .element_children()
            .any(|child| !child.name.eq_ignore_ascii_case("textkeys"))
    {
        return Err(invalid_behavior_source_data(
            "BFBehAnimate playback policy is not yet mapped",
        ));
    }
    let animation_clip_asset_key = animation_action_source_node
        .attribute("targetAnim")
        .filter(|key| !key.trim().is_empty())
        .ok_or_else(|| invalid_behavior_source_data("BFBehAnimate has no targetAnim"))?
        .to_owned();
    let play_duration_seconds = match (
        read_optional_float_attribute(animation_action_source_node, "playTime")?,
        read_optional_float_attribute(animation_action_source_node, "playtime")?,
    ) {
        (Some(_), Some(_)) => {
            return Err(invalid_behavior_source_data(
                "BFBehAnimate authors its play time twice",
            ))
        }
        (seconds, other) => seconds.or(other),
    };
    if play_duration_seconds.is_some_and(|seconds| !seconds.is_finite() || seconds < 0.0) {
        return Err(invalid_behavior_source_data(
            "BFBehAnimate play time is invalid",
        ));
    }
    let blend_duration_seconds =
        read_optional_float_attribute(animation_action_source_node, "blendDuration")?;
    if blend_duration_seconds.is_some_and(|seconds| !seconds.is_finite() || seconds < 0.0) {
        return Err(invalid_behavior_source_data(
            "BFBehAnimate blend duration is invalid",
        ));
    }
    let valid_speed = |speed: f32| speed.is_finite() && speed > 0.0;
    let playback_speed = match (
        read_optional_float_attribute(animation_action_source_node, "animSpeed")?,
        read_optional_float_attribute(animation_action_source_node, "minAnimSpeed")?,
        read_optional_float_attribute(animation_action_source_node, "maxAnimSpeed")?,
    ) {
        (None, None, None) => None,
        (Some(speed), None, None) if valid_speed(speed) => {
            Some(BehaviorAnimationPlaybackSpeed::Fixed(speed))
        }
        (None, Some(minimum), Some(maximum))
            if valid_speed(minimum) && valid_speed(maximum) && minimum <= maximum =>
        {
            Some(BehaviorAnimationPlaybackSpeed::Range { minimum, maximum })
        }
        _ => {
            return Err(invalid_behavior_source_data(
                "BFBehAnimate playback speed is invalid",
            ))
        }
    };
    let ground_fit = animation_action_source_node
        .attribute("groundFit")
        .map(|_| read_boolean_attribute_or_false(animation_action_source_node, "groundFit"))
        .transpose()?;
    let animation = BehaviorAnimationClipAction {
        animation_clip_asset_key,
        looping: read_boolean_attribute_or_false(animation_action_source_node, "loopFlag")?,
        physical_object: animation_action_source_node
            .attribute("physObj")
            .map(str::trim)
            .filter(|name| !name.is_empty())
            .map(str::to_owned),
        play_duration_seconds,
        interrupts_current_animation: read_boolean_attribute_or_false(
            animation_action_source_node,
            "interruptFlag",
        )?,
        interruptible: read_boolean_attribute_or_false(
            animation_action_source_node,
            "interruptible",
        )?,
        ground_fit,
        blend_duration_seconds,
        playback_speed,
    };
    let keys = lower_behavior_animation_text_keys(animation_action_source_node)?;
    Ok(Some(if keys.is_empty() {
        BehaviorAction::AnimationClip(animation)
    } else {
        BehaviorAction::AnimationWithTextKeys { animation, keys }
    }))
}

fn lower_animation_switch(node: &DataNode) -> io::Result<BehaviorAction> {
    let mut tables = node.element_children();
    let table = tables
        .next()
        .filter(|table| table.name == "animTable")
        .ok_or_else(|| invalid_behavior_source_data("BFBehAnimateSwitch requires animTable"))?;
    if !node.attributes.is_empty() || tables.next().is_some() {
        return Err(invalid_behavior_source_data(
            "BFBehAnimateSwitch has unmapped playback policy",
        ));
    }
    let mut action = BehaviorAnimationSwitchAction {
        clips_by_node: Vec::new(),
        default_clip: None,
    };
    for row in table.element_children() {
        let clip = super::source_value_reading::read_optional_trimmed_string_attribute(row, "anim")
            .ok_or_else(|| invalid_behavior_source_data("BFBehAnimateSwitch row requires anim"))?;
        if row.name.eq_ignore_ascii_case("default") {
            action.default_clip = Some(clip);
        } else {
            action.clips_by_node.push((row.name.to_string(), clip));
        }
    }
    Ok(BehaviorAction::AnimationSwitch(action))
}

pub(super) fn lower_behavior_animation_text_keys(
    node: &DataNode,
) -> io::Result<openzt2_game_data::behavior::action_record::BehaviorAnimationTextKeyAliases> {
    use crate::source_document::animation_text_keys::parse_blue_fang_embedded_animation_text_key;
    use openzt2_game_data::AssetId;

    node.element_children()
        .filter(|child| child.name.eq_ignore_ascii_case("textkeys"))
        .flat_map(DataNode::element_children)
        .map(|entry| {
            let text = entry
                .attribute("text")
                .ok_or_else(|| invalid_behavior_source_data("behavior text key has no text"))?;
            let key = parse_blue_fang_embedded_animation_text_key(0.0, text)
                .ok_or_else(|| invalid_behavior_source_data("behavior text key has no commands"))?;
            Ok((
                AssetId::from_key(&entry.name.to_ascii_lowercase()),
                key.animation_text_commands,
            ))
        })
        .collect()
}
