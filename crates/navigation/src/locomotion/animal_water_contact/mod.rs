use bevy::prelude::*;
use openzt2_game_assets::{
    species::species_asset_types::{SpeciesAsset, SpeciesAssets},
    terrain::terrain_asset_types_and_borrowing_queries::TerrainAsset,
};
use openzt2_zoo_state::{
    animal_lifecycle::types::AnimalVariant,
    locomotion::locomotion_types::{AnimalTerrainWaterContact, LocomotionMode, SwimLayer},
    terrain::{
        terrain_chunk_types::{EditedTerrainSamples, TerrainChunk, TerrainIndex},
        terrain_world_sampling::{sample_terrain, terrain_chunk_at},
    },
};

pub(super) fn project_animal_water_contact(
    mut commands: Commands,
    species_assets: Res<Assets<SpeciesAsset>>,
    species_index: Res<SpeciesAssets>,
    terrain_assets: Res<Assets<TerrainAsset>>,
    terrain_index: Res<TerrainIndex>,
    chunks: Query<(&TerrainChunk, Option<&EditedTerrainSamples>)>,
    animals: Query<
        (Entity, &AnimalVariant, &Transform, Option<&LocomotionMode>),
        (
            Without<openzt2_zoo_state::animal_health::types::Dead>,
            Without<openzt2_world_state::world_spawn::world_entity_crating::WorldEntityIsCrated>,
        ),
    >,
) {
    let Some(species) = species_index.get(&species_assets) else {
        return;
    };
    for (entity, variant, transform, current_mode) in &animals {
        let Some((chunk, edited)) = terrain_chunk_at(&terrain_index, transform.translation.xz())
            .and_then(|entity| chunks.get(entity).ok())
        else {
            continue;
        };
        let Some(sample) = terrain_assets
            .get(&chunk.asset)
            .and_then(|asset| sample_terrain(chunk, asset, edited, transform.translation.xz()))
        else {
            continue;
        };
        let depth_m = sample.water_height_m.map(|height| height - sample.height_m);
        commands
            .entity(entity)
            .insert(AnimalTerrainWaterContact { depth_m });
        let Some(policy) = species
            .find_variant(variant.0)
            .and_then(|variant| variant.surface_water_locomotion.as_ref())
        else {
            continue;
        };
        let current = current_mode.copied().unwrap_or_default();
        if !matches!(current, LocomotionMode::Ground | LocomotionMode::Swim) {
            continue;
        }
        let threshold = if current == LocomotionMode::Swim {
            policy.water_exit_depth_m
        } else {
            policy.water_entry_depth_m
        };
        let next = if depth_m.is_some_and(|depth| depth >= threshold) {
            LocomotionMode::Swim
        } else {
            LocomotionMode::Ground
        };
        if current_mode.is_none() || next != current {
            commands.entity(entity).insert(next);
            if next == LocomotionMode::Swim {
                commands.entity(entity).insert(SwimLayer::Surface);
            } else {
                commands.entity(entity).remove::<SwimLayer>();
            }
        }
    }
}
