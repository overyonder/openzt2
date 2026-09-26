use std::io;

use openzt2_game_data::behavior::{
    action::animation::BehaviorAnimationClipAction, action_record::BehaviorAction,
};

use crate::assets::source_document::ordered_source_document_types::OrderedSourceDocumentNode as DataNode;

use super::{invalid_behavior_source_data, source_value_reading::read_boolean_attribute_or_false};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assets::source_document::{
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
    if animation_action_source_node.name != "BFBehAnimate" {
        return Ok(None);
    }
    if animation_action_source_node
        .attributes
        .iter()
        .any(|attribute| !matches!(attribute.name(), "targetAnim" | "loopFlag"))
        || animation_action_source_node
            .element_children()
            .next()
            .is_some()
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
    Ok(Some(BehaviorAction::AnimationClip(
        BehaviorAnimationClipAction {
            animation_clip_asset_key,
            looping: read_boolean_attribute_or_false(animation_action_source_node, "loopFlag")?,
        },
    )))
}
