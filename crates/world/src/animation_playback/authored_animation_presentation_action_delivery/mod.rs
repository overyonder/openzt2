use std::sync::Arc;

use bevy::prelude::*;
use openzt2_game_data::animation::animation_text_key::AuthoredAnimationTextAction;

use super::authored_animation_text_action_execution_context::AuthoredAnimationAudioAndEffectTextActionExecutionContext;

#[derive(Message)]
pub(super) struct AnimationPresentationTextAction<const REAL_TIME: bool> {
    pub(super) target: Entity,
    pub(super) milliseconds: u32,
    pub(super) action: Arc<AuthoredAnimationTextAction>,
}

pub(super) fn deliver_presentation_text_actions<const REAL_TIME: bool>(
    mut messages: MessageReader<AnimationPresentationTextAction<REAL_TIME>>,
    mut effects: AuthoredAnimationAudioAndEffectTextActionExecutionContext,
) {
    for message in messages.read() {
        let resolved_action_target_entity = message.target;
        let event_time_milliseconds = message.milliseconds;
        match &*message.action {
            AuthoredAnimationTextAction::PlaySound {
                audio_asset_key,
                use_terrain_surface_sound,
                playback_is_looped,
                update_sound_position_during_playback,
                ..
            } => effects.play_authored_animation_sound(
                resolved_action_target_entity,
                event_time_milliseconds,
                audio_asset_key,
                *playback_is_looped,
                *update_sound_position_during_playback,
                *use_terrain_surface_sound,
            ),
            AuthoredAnimationTextAction::RunParticleSystem {
                particle_system_asset_key,
                particle_scale,
                use_target_rotation,
                use_target_z_axis_rotation,
            } => effects.run_authored_animation_particle_system(
                resolved_action_target_entity,
                particle_system_asset_key,
                particle_scale.unwrap_or(1.0),
                *use_target_rotation,
                *use_target_z_axis_rotation,
            ),
            AuthoredAnimationTextAction::AttachParticleSystem {
                particle_system_asset_key,
                use_target_rotation,
                particle_scale,
            } => effects.attach_authored_animation_particle_system(
                resolved_action_target_entity,
                particle_system_asset_key,
                *use_target_rotation,
                particle_scale.unwrap_or(1.0),
            ),
            AuthoredAnimationTextAction::RunPresentationEffect {
                presentation_effect_asset_key,
                attach_to_target,
                stop_attached_effects: false,
                water_height_offset,
                disable_distance_culling,
            } => effects.run_authored_animation_presentation_effect(
                resolved_action_target_entity,
                presentation_effect_asset_key,
                *attach_to_target,
                *water_height_offset,
                *disable_distance_culling,
            ),
            AuthoredAnimationTextAction::DetachParticleSystem {
                particle_system_asset_key: effect_asset_key,
            }
            | AuthoredAnimationTextAction::RunPresentationEffect {
                presentation_effect_asset_key: effect_asset_key,
                stop_attached_effects: true,
                ..
            } => effects.stop_authored_animation_effects_attached_to_target(
                resolved_action_target_entity,
                effect_asset_key,
            ),
            _ => unreachable!("only presentation actions are queued here"),
        }
    }
}
