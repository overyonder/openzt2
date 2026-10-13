use bevy::ecs::system::SystemParam;
use bevy::prelude::*;

use openzt2_game_assets::effect::SpawnCompleteParticleEffectDocument;
use openzt2_game_assets::effect::StopParticleEffectsAttachedToEntity;
use openzt2_world_state::animation_graph::animation_graph_playback_message_types::AnimationGraphNodePlaybackRequest;
use openzt2_zoo_state::audio::audio_playback_message_types::PlayAudioCue;

use openzt2_world_state::animation_playback::animation_event_message_types::AnimationTraversalTransitionRequested;
use openzt2_world_state::animation_playback::animation_joint_target_types::AnimationJointTarget;

#[derive(SystemParam)]
pub(super) struct AuthoredAnimationTextActionExecutionContext<'w, 's> {
    pub(super) object_commands:
        MessageWriter<'w, openzt2_world_state::animation_playback::animation_event_message_types::AnimationObjectCommand>,
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
    pub(super) audio_messages: MessageWriter<'w, PlayAudioCue>,
    pub(super) effect_messages: MessageWriter<'w, SpawnCompleteParticleEffectDocument>,
    pub(super) stop_effect_messages: MessageWriter<'w, StopParticleEffectsAttachedToEntity>,
}

#[derive(SystemParam)]
pub(super) struct AuthoredAnimationControlTextActionExecutionContext<'w, 's> {
    pub(super) commands: Commands<'w, 's>,
    pub(super) animation_node_messages: MessageWriter<'w, AnimationGraphNodePlaybackRequest>,
    pub(super) traversal_messages: MessageWriter<'w, AnimationTraversalTransitionRequested>,
}
