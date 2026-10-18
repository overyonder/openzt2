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
        AuthoredAnimationAudioAndEffectTextActionExecutionContext<'w, 's>,
    pub(super) animation_control_text_action_execution:
        AuthoredAnimationControlTextActionExecutionContext<'w, 's>,
}

#[derive(SystemParam)]
pub(super) struct AuthoredAnimationAudioAndEffectTextActionExecutionContext<'w, 's> {
    pub(super) commands: Commands<'w, 's>,
    pub(super) terrain_index: Res<'w, openzt2_zoo_state::terrain::terrain_chunk_types::TerrainIndex>,
    pub(super) terrain_assets: Res<'w, Assets<openzt2_game_assets::terrain::terrain_asset_types_and_borrowing_queries::TerrainAsset>>,
    pub(super) terrain_chunks: Query<'w, 's, &'static ViewVisibility, With<openzt2_zoo_state::terrain::terrain_chunk_types::TerrainChunk>>,
    pub(super) water_regions: openzt2_zoo_state::terrain::terrain_water_presentation_types::TerrainWaterRegionQuery<'w, 's>,
    pub(super) water_memberships: Query<'w, 's, &'static openzt2_zoo_state::terrain::terrain_water_presentation_types::TerrainWaterMembershipIndex>,
    pub(super) asset_server: Res<'w, AssetServer>,
    pub(super) transforms: Query<'w, 's, &'static GlobalTransform>,
    pub(super) sound_resolution: super::authored_animation_sound_resolution::AuthoredAnimationSoundResolution<'w, 's>,
    pub(super) audio_messages: MessageWriter<'w, PlayAudioCue>,
    pub(super) positioned_audio_messages: MessageWriter<'w, openzt2_zoo_state::audio::audio_playback_message_types::PlayPositionedAudioCue>,
    pub(super) effect_messages: MessageWriter<'w, SpawnCompleteParticleEffectDocument>,
    pub(super) stop_effect_messages: MessageWriter<'w, StopParticleEffectsAttachedToEntity>,
}

#[derive(SystemParam)]
pub(super) struct AuthoredAnimationControlTextActionExecutionContext<'w, 's> {
    pub(super) commands: Commands<'w, 's>,
    pub(super) animation_node_messages: MessageWriter<'w, AnimationGraphNodePlaybackRequest>,
    pub(super) traversal_messages: MessageWriter<'w, AnimationTraversalTransitionRequested>,
}
