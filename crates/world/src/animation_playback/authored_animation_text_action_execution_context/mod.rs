use bevy::ecs::system::SystemParam;
use bevy::prelude::*;

use openzt2_game_assets::effect::SpawnCompleteParticleEffectDocument;
use openzt2_game_assets::effect::StopParticleEffectsAttachedToEntity;
use openzt2_world_state::animation_graph::animation_graph_playback_message_types::AnimationGraphNodePlaybackRequest;
use openzt2_world_state::animation_graph::animation_graph_playback_state_types::AnimationGraphEnabled;
use openzt2_zoo_state::audio::audio_playback_message_types::PlayAudioCue;

use openzt2_world_state::animation_playback::animation_event_message_types::AnimationTraversalTransitionRequested;
use openzt2_world_state::animation_playback::animation_event_message_types::{
    AnimationAiCommandRequested, AnimationWhapRequested,
};
use openzt2_world_state::animation_playback::animation_joint_target_types::{
    AnimationJointTarget, AnimationRigJoints,
};

#[derive(SystemParam)]
pub(super) struct AuthoredAnimationTextActionExecutionContext<'w, 's, const REAL_TIME: bool> {
    pub(super) binder_commands: MessageWriter<'w, super::AnimationNamedPhysicalPresentationCommand<REAL_TIME>>,
    pub(super) object_commands:
        MessageWriter<'w, openzt2_world_state::animation_playback::animation_event_message_types::AnimationObjectCommand>,
    pub(super) named_animation_joint_targets:
        Query<'w, 's, (Entity, &'static AnimationJointTarget)>,
    pub(super) animation_rig_joints: Query<'w, 's, &'static AnimationRigJoints>,
    pub(super) presentation_actions: MessageWriter<'w, super::authored_animation_presentation_action_delivery::AnimationPresentationTextAction<REAL_TIME>>,
    pub(super) animation_control_text_action_execution:
        AuthoredAnimationControlTextActionExecutionContext<'w, 's>,
}

#[derive(SystemParam)]
pub(super) struct AuthoredAnimationAudioAndEffectTextActionExecutionContext<'w, 's> {
    pub(super) commands: Commands<'w, 's>,
    pub(super) terrain_index:
        Res<'w, openzt2_zoo_state::terrain::terrain_chunk_types::TerrainIndex>,
    pub(super) terrain_chunks: Query<
        'w,
        's,
        &'static ViewVisibility,
        With<openzt2_zoo_state::terrain::terrain_chunk_types::TerrainChunk>,
    >,
    pub(super) asset_server: Res<'w, AssetServer>,
    pub(super) transforms: Query<'w, 's, &'static GlobalTransform>,
    pub(super) sound_resolution:
        super::authored_animation_sound_resolution::AuthoredAnimationSoundResolution<'w, 's>,
    pub(super) audio_messages: MessageWriter<'w, PlayAudioCue>,
    pub(super) positioned_audio_messages: MessageWriter<
        'w,
        openzt2_zoo_state::audio::audio_playback_message_types::PlayPositionedAudioCue,
    >,
    pub(super) effect_messages: MessageWriter<'w, SpawnCompleteParticleEffectDocument>,
    pub(super) stop_effect_messages: MessageWriter<'w, StopParticleEffectsAttachedToEntity>,
}

#[derive(SystemParam)]
pub(super) struct AuthoredAnimationControlTextActionExecutionContext<'w, 's> {
    pub(super) commands: Commands<'w, 's>,
    pub(super) graphs_enabled: Query<'w, 's, &'static mut AnimationGraphEnabled>,
    pub(super) ground_fit: Query<'w, 's, &'static mut openzt2_world_state::animation_playback::animation_playback_controller_types::AnimationGroundFitControls>,
    pub(super) ai_commands: MessageWriter<'w, AnimationAiCommandRequested>,
    pub(super) decal_cycles: Query<'w, 's, &'static mut super::authored_decal_texture_cycles::DecalCyclePlayback>,
    pub(super) whap_requests: MessageWriter<'w, AnimationWhapRequested>,
    pub(super) animation_node_messages: MessageWriter<'w, AnimationGraphNodePlaybackRequest>,
    pub(super) traversal_messages: MessageWriter<'w, AnimationTraversalTransitionRequested>,
}
