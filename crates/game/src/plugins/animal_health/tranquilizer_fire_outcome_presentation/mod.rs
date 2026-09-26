use bevy::prelude::*;

use crate::assets::effect::SpawnCompleteParticleEffectDocument;
use crate::assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use crate::assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use crate::plugins::audio::audio_playback_message_types::PlayAudioCue;
use crate::plugins::camera::camera_runtime_state_types::ZooCamera;

use super::types::{TranquilizerFireOutcome, TranquilizerFired};

pub(super) fn present_tranquilizer_fire_audio_and_particle_effects(
    mut fired_messages: MessageReader<TranquilizerFired>,
    world_definition_assets: Res<Assets<WorldDefinitionAsset>>,
    active_world_definitions: Res<WorldDefinitions>,
    cameras: Query<(Entity, &GlobalTransform), With<ZooCamera>>,
    mut audio_requests: MessageWriter<PlayAudioCue>,
    mut particle_effect_requests: MessageWriter<SpawnCompleteParticleEffectDocument>,
) {
    let Some(world_definitions) = active_world_definitions.get(&world_definition_assets) else {
        return;
    };
    let Some(tranquilizer_mode) = world_definitions.tranquilizer_mode() else {
        return;
    };
    for fired in fired_messages.read() {
        if world_definitions
            .find_tranquilizer(fired.tranquilizer)
            .is_none()
        {
            continue;
        }
        let audio_cue = match fired.outcome {
            TranquilizerFireOutcome::Shot => tranquilizer_mode.shot_cue,
            TranquilizerFireOutcome::Misfire => tranquilizer_mode.misfire_cue,
            TranquilizerFireOutcome::NoTarget | TranquilizerFireOutcome::OutOfRange => continue,
        };
        audio_requests.write(PlayAudioCue {
            cue: audio_cue,
            emitter: Some(fired.controller),
            selection: fired.controller.to_bits(),
            priority: 192,
            force_looped: false,
        });
        if fired.outcome != TranquilizerFireOutcome::Shot {
            continue;
        }
        let particle_effect_identifier = tranquilizer_mode.shot_effect;
        let Some(particle_effect_asset) = world_definitions.effect(particle_effect_identifier)
        else {
            continue;
        };
        let Ok((camera, _)) = cameras.single() else {
            continue;
        };
        particle_effect_requests.write(SpawnCompleteParticleEffectDocument {
            particle_effect_asset: particle_effect_asset.clone(),
            parent_entity: Some(camera),
            effect_transform: Transform::from_translation(Vec3::new(
                tranquilizer_mode.shot_effect_offset_cm[0] as f32 / 100.0,
                tranquilizer_mode.shot_effect_offset_cm[1] as f32 / 100.0,
                -(tranquilizer_mode.shot_effect_distance_cm as f32 / 100.0),
            )),
            manual_particle_count: 1,
        });
    }
}
