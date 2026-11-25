use bevy::prelude::*;
use openzt2_game_assets::species::species_asset_types::{SpeciesAsset, SpeciesAssets};
use openzt2_game_assets::terrain::terrain_asset_types_and_borrowing_queries::TerrainAsset;
use openzt2_world_state::world_spawn::world_entity_crating::WorldEntityIsCrated;
use openzt2_world_state::world_spawn::world_membership_types::WorldMember;
use openzt2_zoo_state::{
    animal_lifecycle::types::AnimalVariant,
    aquatic::aquatic_simulation_types::TankGeometry,
    habitat::habitat_types::HabitatIndex,
    locomotion::locomotion_types::{AnimalTerrainWaterContact, LocomotionMode, SwimLayer},
    terrain::dry_surface_eligibility::DrySurfaceEligibility,
    terrain::{
        terrain_chunk_types::{TerrainChunk, TerrainIndex},
        terrain_navigation_change_types::{TerrainNavigationChanged, TerrainNavigationRestored},
        terrain_water_presentation_types::{
            TerrainWaterMembershipIndex, TerrainWaterRegionEdit, TerrainWaterRegionRetired,
            TerrainWaterRegionSource,
        },
    },
};

#[derive(SystemParam)]
pub(super) struct AnimalWaterChanges<'w, 's> {
    terrain_assets: Res<'w, Assets<TerrainAsset>>,
    terrain_refresh: Query<
        'w,
        's,
        (),
        Or<(
            Changed<TerrainWaterMembershipIndex>,
            Changed<TerrainWaterRegionEdit>,
            Changed<TerrainWaterRegionSource>,
            Added<TerrainWaterRegionRetired>,
            Changed<TerrainIndex>,
        )>,
    >,
    tanks: Query<'w, 's, &'static WorldMember, Changed<TankGeometry>>,
    habitats: Query<'w, 's, (), Changed<HabitatIndex>>,
    chunks: Query<'w, 's, (&'static TerrainChunk, &'static WorldMember)>,
    edits: MessageReader<'w, 's, TerrainNavigationChanged>,
    restores: MessageReader<'w, 's, TerrainNavigationRestored>,
    removed_tanks: RemovedComponents<'w, 's, TankGeometry>,
    removed_water: RemovedComponents<'w, 's, TerrainWaterRegionEdit>,
    removed_indexes: RemovedComponents<'w, 's, TerrainWaterMembershipIndex>,
    removed_source: RemovedComponents<'w, 's, TerrainWaterRegionSource>,
    removed_retirement: RemovedComponents<'w, 's, TerrainWaterRegionRetired>,
    resumed: RemovedComponents<'w, 's, WorldEntityIsCrated>,
    revived: RemovedComponents<'w, 's, Dead>,
    changed_modes: RemovedComponents<'w, 's, LocomotionMode>,
    enabled: RemovedComponents<'w, 's, bevy::ecs::entity_disabling::Disabled>,
}

#[allow(clippy::type_complexity)]
pub(super) fn project_animal_water_contact(
    mut commands: Commands,
    species_assets: Res<Assets<SpeciesAsset>>,
    species_index: Res<SpeciesAssets>,
    surfaces: DrySurfaceEligibility,
    mut changes: AnimalWaterChanges,
    mut animals: Query<
        (
            Entity,
            Ref<WorldMember>,
            Ref<AnimalVariant>,
            &mut Transform,
            Option<&mut LocomotionMode>,
            Option<&mut AnimalTerrainWaterContact>,
        ),
        (Without<Dead>, Without<WorldEntityIsCrated>),
    >,
) {
    let refresh_all = species_index.is_changed()
        | species_assets.is_changed()
        | changes.terrain_assets.is_changed()
        | !changes.terrain_refresh.is_empty()
        | (changes.removed_tanks.read().count() != 0)
        | (changes.removed_water.read().count() != 0)
        | (changes.removed_indexes.read().count() != 0)
        | (changes.removed_source.read().count() != 0)
        | (changes.removed_retirement.read().count() != 0);
    let Some(species) = species_index.get(&species_assets) else {
        return;
    };
    let edited: Vec<(Entity, Rect)> = changes
        .edits
        .read()
        .map(|edit| (edit.chunk, edit.min, edit.max))
        .chain(
            changes
                .restores
                .read()
                .map(|edit| (edit.chunk, edit.min, edit.max)),
        )
        .filter_map(|(chunk, min, max)| {
            let (chunk, member) = changes.chunks.get(chunk).ok()?;
            Some((
                member.root,
                super::terrain_edit_navigation_refresh::navigation_sample_edit_bounds(
                    chunk, min, max,
                ),
            ))
        })
        .collect();
    let changed_tanks: EntityHashSet = changes.tanks.iter().map(|member| member.root).collect();
    let resumed: EntityHashSet = changes
        .resumed
        .read()
        .chain(changes.revived.read())
        .chain(changes.changed_modes.read())
        .chain(changes.enabled.read())
        .collect();
    for (entity, member, variant, mut transform, current_mode, contact) in &mut animals {
        let position = transform.translation.xz();
        if !refresh_all
            && !member.is_changed()
            && !variant.is_changed()
            && !transform.is_changed()
            && !current_mode.as_ref().is_some_and(DetectChanges::is_changed)
            && contact.is_some()
            && !resumed.contains(&entity)
            && !changes.habitats.contains(member.root)
            && !changed_tanks.contains(&member.root)
            && !edited
                .iter()
                .any(|(root, bounds)| *root == member.root && bounds.contains(position))
        {
            continue;
        }
        let Some(ground) = surfaces.ground_height(member.root, position) else {
            continue;
        };
        let surface = surfaces.water_surface_height(member.root, position);
        let depth_m = surface.map(|height| height - ground);
        if let Some(contact) = contact {
            contact
                .map_unchanged(|contact| &mut contact.depth_m)
                .set_if_neq(depth_m);
        } else {
            commands
                .entity(entity)
                .insert(AnimalTerrainWaterContact { depth_m });
        }
        let Some(policy) = species
            .find_variant(variant.0)
            .and_then(|variant| variant.surface_water_locomotion.as_ref())
        else {
            continue;
        };
        let current = current_mode.as_deref().copied().unwrap_or_default();
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
        // Motion fits a swimmer to the ground under a tank, whose water is not
        // terrain water; a leap above the surface stays where it is.
        if let Some(surface) = surface.filter(|_| next == LocomotionMode::Swim) {
            if transform.translation.y < surface {
                transform.translation.y = surface;
            }
        }
        if current_mode.is_none() || next != current {
            if let Some(mut mode) = current_mode {
                *mode = next;
            } else {
                commands.entity(entity).insert(next);
            }
            if next == LocomotionMode::Swim {
                commands.entity(entity).insert(SwimLayer::Surface);
            } else {
                commands.entity(entity).remove::<SwimLayer>();
            }
        }
    }
}
