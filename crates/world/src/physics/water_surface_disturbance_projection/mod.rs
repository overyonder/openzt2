use bevy::prelude::*;
use openzt2_game_assets::terrain::terrain_asset_types_and_borrowing_queries::TerrainAsset;
use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use openzt2_game_assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use openzt2_game_data::animation::animation_text_key::{
    AuthoredAnimationSurfaceEffectKind, AuthoredAnimationTextAction,
};
use openzt2_world_state::animation_playback::animation_event_message_types::AnimationObjectCommand;
use openzt2_world_state::world_spawn::world_membership_types::DefinitionId;
use openzt2_zoo_state::terrain::terrain_chunk_types::{EditedTerrainSamples, TerrainChunk, TerrainIndex};
use openzt2_zoo_state::terrain::terrain_water_geometric_wave_shader_state::{
    ActivateTerrainWaterImpactWave, WaterSurfaceSpraySample, WaterSurfaceWakeSample,
};
use openzt2_zoo_state::terrain::terrain_water_presentation_types::{TerrainWaterMembershipIndex, TerrainWaterRegionQuery};
use super::authored_water_surface_impact_projection::sample_accepted_water_surface;

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub(crate) fn project_animation_surface_effects_into_water_disturbances(
    mut requests: MessageReader<AnimationObjectCommand>,
    transforms: Query<&GlobalTransform>,
    definitions: Query<&DefinitionId>,
    parents: Query<&ChildOf>,
    definition_assets: Res<Assets<WorldDefinitionAsset>>,
    active_definitions: Res<WorldDefinitions>,
    terrain_index: Option<Res<TerrainIndex>>,
    terrain_assets: Res<Assets<TerrainAsset>>,
    chunks: Query<(&TerrainChunk, Option<&EditedTerrainSamples>)>,
    memberships: Query<&TerrainWaterMembershipIndex>,
    regions: TerrainWaterRegionQuery,
    mut impacts: MessageWriter<ActivateTerrainWaterImpactWave>,
    mut spray: MessageWriter<WaterSurfaceSpraySample>,
    mut wakes: MessageWriter<WaterSurfaceWakeSample>,
) {
    let Some(index) = terrain_index else { return };
    let Some(definition_view) = active_definitions.get(&definition_assets) else {
        return;
    };
    for request in requests.read() {
        let AuthoredAnimationTextAction::TriggerSurfaceEffect {
            surface_effect_kind,
            effect_strength,
            ..
        } = &request.action
        else {
            continue;
        };
        if *surface_effect_kind == AuthoredAnimationSurfaceEffectKind::NamedSurfaceEffect {
            continue;
        }
        let Ok(joint_transform) = transforms.get(request.joint) else {
            continue;
        };
        let mut owner = request.controller;
        let definition = loop {
            if let Ok(definition) = definitions.get(owner) {
                break Some(definition);
            }
            let Ok(parent) = parents.get(owner) else {
                break None;
            };
            owner = parent.parent();
        };
        let authored_strength = definition
            .and_then(|definition| definition_view.find_object(definition.0))
            .and_then(|definition| definition.surface_effect_strength);
        let strength = effect_strength
            .filter(|strength| *strength >= 0.0)
            .or(authored_strength)
            .unwrap_or(1.0);
        let position = joint_transform.translation().xz();
        let Some((_, surface)) = sample_accepted_water_surface(
            &index,
            &terrain_assets,
            &chunks,
            &memberships,
            &regions,
            position,
        ) else {
            continue;
        };
        let heading = joint_transform.affine().transform_vector3(Vec3::NEG_Z);
        let direction = Vec3::new(heading.x, 0.0, heading.z);
        let source_position = Vec2::new(position.x, -position.y);
        if matches!(
            surface_effect_kind,
            AuthoredAnimationSurfaceEffectKind::SurfaceImpact
                | AuthoredAnimationSurfaceEffectKind::DirectionalVectorAndSurfaceImpact
        ) {
            wakes.write(WaterSurfaceWakeSample {
                surface,
                source_position,
                direction: Vec2::new(direction.x, -direction.z),
                native_strength: strength,
            });
            impacts.write(ActivateTerrainWaterImpactWave {
                surface,
                source_position,
                native_strength: strength,
            });
        }
        if matches!(
            surface_effect_kind,
            AuthoredAnimationSurfaceEffectKind::DirectionalVector
                | AuthoredAnimationSurfaceEffectKind::DirectionalVectorAndSurfaceImpact
        ) {
            spray.write(WaterSurfaceSpraySample {
                surface,
                source_position,
                direction,
                normal: Vec3::Y,
                native_strength: strength,
            });
        }
    }
}
