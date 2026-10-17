use bevy::prelude::*;

use crate::plugins::audio::audio_playback_message_types::PlayAudioCue;
use openzt2_game_assets::audio::source::xml::cue_id;
use openzt2_game_assets::effect::{
    ParticleEffectDocumentAsset, SpawnCompleteParticleEffectDocument,
    StopParticleEffectsAttachedToEntity,
};

use super::authored_animation_text_action_execution_context::AuthoredAnimationAudioAndEffectTextActionExecutionContext;

impl AuthoredAnimationAudioAndEffectTextActionExecutionContext<'_> {
    pub(super) fn play_authored_animation_sound(
        &mut self,
        animation_playback_controller_entity: Entity,
        event_time_milliseconds: u32,
        audio_cue_name: &str,
        audio_loops: bool,
        update_audio_position: bool,
    ) {
        // `snd Growl` names a sound cue from the audio documents, not a file.
        self.audio_messages.write(PlayAudioCue {
            cue: cue_id(audio_cue_name),
            emitter: update_audio_position.then_some(animation_playback_controller_entity),
            selection: event_time_milliseconds as u64,
            priority: 128,
            force_looped: audio_loops,
        });
    }

    pub(super) fn run_authored_animation_particle_system(
        &mut self,
        effect_target_entity: Entity,
        effect_asset_path: &str,
        uniform_scale: f32,
    ) {
        self.effect_messages
            .write(SpawnCompleteParticleEffectDocument {
                particle_effect_asset: self
                    .asset_server
                    .load::<ParticleEffectDocumentAsset>(effect_asset_path.to_owned()),
                parent_entity: Some(effect_target_entity),
                effect_transform: Transform::from_scale(Vec3::splat(uniform_scale)),
                manual_particle_count: 0,
            });
    }

    pub(super) fn attach_authored_animation_particle_system(
        &mut self,
        effect_target_entity: Entity,
        effect_asset_path: &str,
    ) {
        self.effect_messages
            .write(SpawnCompleteParticleEffectDocument {
                particle_effect_asset: self
                    .asset_server
                    .load::<ParticleEffectDocumentAsset>(effect_asset_path.to_owned()),
                parent_entity: Some(effect_target_entity),
                effect_transform: Transform::default(),
                manual_particle_count: 0,
            });
    }

    pub(super) fn run_authored_animation_presentation_effect(
        &mut self,
        effect_target_entity: Entity,
        effect_asset_path: &str,
        attach_to_target: bool,
        water_vertical_offset: f32,
    ) {
        self.effect_messages
            .write(SpawnCompleteParticleEffectDocument {
                particle_effect_asset: self
                    .asset_server
                    .load::<ParticleEffectDocumentAsset>(effect_asset_path.to_owned()),
                parent_entity: attach_to_target.then_some(effect_target_entity),
                effect_transform: Transform::from_xyz(0.0, water_vertical_offset, 0.0),
                manual_particle_count: 0,
            });
    }

    pub(super) fn stop_authored_animation_effects_attached_to_target(
        &mut self,
        effect_target_entity: Entity,
    ) {
        self.stop_effect_messages
            .write(StopParticleEffectsAttachedToEntity {
                attached_entity: effect_target_entity,
            });
    }
}
