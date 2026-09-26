use bevy::{ecs::system::SystemParam, prelude::*};

use crate::{
    assets::effect::{SpawnCompleteParticleEffectDocument, StopParticleEffectsAttachedToEntity},
    plugins::{
        animation_graph::animation_graph_playback_message_types::AnimationGraphNodePlaybackRequest,
        audio::audio_playback_message_types::PlayAudioClip,
    },
};

use super::{
    animation_event_message_types::AnimationTraversalTransitionRequested,
    animation_joint_target_types::AnimationJointTarget,
};

#[derive(SystemParam)]
pub(super) struct AuthoredAnimationTextActionExecutionContext<'w, 's> {
    pub(super) object_commands:
        MessageWriter<'w, super::animation_event_message_types::AnimationObjectCommand>,
    pub(super) named_animation_joint_targets:
        Query<'w, 's, (Entity, &'static AnimationJointTarget)>,
    pub(super) audio_and_effect_text_action_execution:
        AuthoredAnimationAudioAndEffectTextActionExecutionContext<'w>,
    pub(super) animation_control_text_action_execution:
        AuthoredAnimationControlTextActionExecutionContext<'w, 's>,
}

#[derive(SystemParam)]
pub(super) struct AuthoredAnimationAudioAndEffectTextActionExecutionContext<'w> {
    pub(super) asset_server: Res<'w, AssetServer>,
    pub(super) audio_messages: MessageWriter<'w, PlayAudioClip>,
    pub(super) effect_messages: MessageWriter<'w, SpawnCompleteParticleEffectDocument>,
    pub(super) stop_effect_messages: MessageWriter<'w, StopParticleEffectsAttachedToEntity>,
}

#[derive(SystemParam)]
pub(super) struct AuthoredAnimationControlTextActionExecutionContext<'w, 's> {
    pub(super) commands: Commands<'w, 's>,
    pub(super) animation_node_messages: MessageWriter<'w, AnimationGraphNodePlaybackRequest>,
    pub(super) traversal_messages: MessageWriter<'w, AnimationTraversalTransitionRequested>,
}
