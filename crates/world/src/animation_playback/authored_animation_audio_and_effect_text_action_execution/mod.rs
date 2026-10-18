use bevy::prelude::*;

use openzt2_game_assets::effect::ParticleEffectDocumentAsset;
use openzt2_game_assets::effect::SpawnCompleteParticleEffectDocument;
use openzt2_game_assets::effect::StopParticleEffectsAttachedToEntity;
use openzt2_zoo_state::audio::audio_playback_message_types::PlayAudioCue;
use openzt2_zoo_state::audio::audio_playback_message_types::PlayPositionedAudioCue;

use crate::animation_playback::authored_animation_text_action_execution_context::AuthoredAnimationAudioAndEffectTextActionExecutionContext;

impl AuthoredAnimationAudioAndEffectTextActionExecutionContext<'_, '_> {
    pub(super) fn play_authored_animation_sound(
        &mut self,
        animation_playback_controller_entity: Entity,
        event_time_milliseconds: u32,
        audio_cue_name: &str,
        audio_loops: bool,
        update_audio_position: bool,
        terrain_sound: bool,
    ) {
        let Ok(transform) = self.transforms.get(animation_playback_controller_entity) else {
            return;
        };
        let position = transform.translation();
        let Some(cue) = self.sound_resolution.resolve(
            animation_playback_controller_entity, position, audio_cue_name, terrain_sound,
        ) else {
            return;
        };
        if update_audio_position {
            self.audio_messages.write(PlayAudioCue {
                cue,
                emitter: Some(animation_playback_controller_entity),
                selection: u64::from(event_time_milliseconds),
                priority: 128,
                force_looped: audio_loops,
            });
        } else {
            self.positioned_audio_messages.write(PlayPositionedAudioCue {
                cue,
                world_position: position,
                selection: u64::from(event_time_milliseconds),
                priority: 128,
                force_looped: audio_loops,
                listener_above_water: true,
                listener_below_water: true,
                admission: None,
            });
        }
    }

    pub(super) fn run_authored_animation_particle_system(
        &mut self,
        effect_target_entity: Entity,
        effect_asset_path: &str,
        uniform_scale: f32,
        use_rotation: bool,
        use_z_rotation: bool,
    ) {
        let Ok(target) = self.transforms.get(effect_target_entity) else {
            return;
        };
        let mut transform = target.compute_transform();
        transform.scale *= uniform_scale;
        transform.rotation = if use_rotation {
            transform.rotation
        } else if use_z_rotation {
            let forward = transform.rotation * Vec3::Z;
            Quat::from_rotation_y(forward.x.atan2(forward.z))
        } else {
            Quat::IDENTITY
        };
        self.effect_messages
            .write(SpawnCompleteParticleEffectDocument {
                particle_effect_asset: self
                    .asset_server
                    .load::<ParticleEffectDocumentAsset>(effect_asset_path.to_owned()),
                parent_entity: None,
                inherit_parent_rotation: true,
                effect_transform: transform,
                manual_particle_count: 0,
            });
    }

    pub(super) fn attach_authored_animation_particle_system(
        &mut self,
        effect_target_entity: Entity,
        effect_asset_path: &str,
        use_target_rotation: bool,
        uniform_scale: f32,
    ) {
        self.effect_messages
            .write(SpawnCompleteParticleEffectDocument {
                particle_effect_asset: self
                    .asset_server
                    .load::<ParticleEffectDocumentAsset>(effect_asset_path.to_owned()),
                parent_entity: Some(effect_target_entity),
                inherit_parent_rotation: use_target_rotation,
                effect_transform: Transform::from_scale(Vec3::splat(uniform_scale)),
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
        let Ok(target) = self.transforms.get(effect_target_entity) else {
            return;
        };
        let transform = if attach_to_target {
            Transform::from_xyz(0.0, water_vertical_offset, 0.0)
        } else {
            let mut transform = target.compute_transform();
            transform.translation.y += water_vertical_offset;
            transform
        };
        self.effect_messages
            .write(SpawnCompleteParticleEffectDocument {
                particle_effect_asset: self
                    .asset_server
                    .load::<ParticleEffectDocumentAsset>(effect_asset_path.to_owned()),
                parent_entity: attach_to_target.then_some(effect_target_entity),
                inherit_parent_rotation: true,
                effect_transform: transform,
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
