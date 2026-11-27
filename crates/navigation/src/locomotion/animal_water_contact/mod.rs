use bevy::{
    ecs::{entity::EntityHashSet, system::SystemParam},
    prelude::*,
};
use openzt2_game_assets::species::species_asset_types::{SpeciesAsset, SpeciesAssets};
use openzt2_game_assets::terrain::terrain_asset_types_and_borrowing_queries::TerrainAsset;
use openzt2_world_state::world_spawn::world_entity_crating::{
    AnimalCrate, ScriptedCrating, UncratedWorldEntity,
};
use openzt2_world_state::world_spawn::world_membership_types::WorldMember;
use openzt2_zoo_state::animal_health::types::Dead;
use openzt2_zoo_state::{
    animal_lifecycle::types::AnimalVariant,
    aquatic::aquatic_simulation_types::{TankGeometry, TankWaterCellsChanged},
    habitat::{
        habitat_cell_ownership_changes::HabitatCellOwnershipChanged,
        habitat_topology_cell_queries::convert_finite_world_position_to_topology_cell,
    },
    locomotion::locomotion_types::{
        AnimalFloatingFallback, AnimalTerrainWaterContact, LocomotionMode, SwimLayer,
    },
    terrain::dry_surface_eligibility::DrySurfaceEligibility,
    terrain::{
        terrain_chunk_types::{TerrainChunk, TerrainIndex},
        terrain_navigation_change_types::{TerrainNavigationChanged, TerrainNavigationRestored},
        terrain_water_presentation_types::{
            TerrainWaterCellsChanged, TerrainWaterMembershipIndex, TerrainWaterRegionEdit,
            TerrainWaterRegionRetired, TerrainWaterRegionSource,
        },
    },
    topology::topology_graph_types::TopologyGrid,
};

#[derive(SystemParam)]
pub(super) struct AnimalWaterChanges<'w, 's> {
    terrain_assets: Res<'w, Assets<TerrainAsset>>,
    terrain_refresh: Query<
        'w,
        's,
        (Entity, Option<&'static WorldMember>),
        Or<(
            Changed<TerrainWaterMembershipIndex>,
            Changed<TerrainWaterRegionEdit>,
            Changed<TerrainWaterRegionSource>,
            Added<TerrainWaterRegionRetired>,
            Changed<TerrainIndex>,
        )>,
    >,
    tanks: Query<'w, 's, (Entity, &'static WorldMember), Changed<TankGeometry>>,
    terrain_water_cells: MessageReader<'w, 's, TerrainWaterCellsChanged>,
    habitat_cells: MessageReader<'w, 's, HabitatCellOwnershipChanged>,
    tank_cells: MessageReader<'w, 's, TankWaterCellsChanged>,
    grids: Query<'w, 's, &'static TopologyGrid>,
    chunks: Query<'w, 's, (&'static TerrainChunk, &'static WorldMember)>,
    edits: MessageReader<'w, 's, TerrainNavigationChanged>,
    restores: MessageReader<'w, 's, TerrainNavigationRestored>,
    removed_tanks: RemovedComponents<'w, 's, TankGeometry>,
    removed_water: RemovedComponents<'w, 's, TerrainWaterRegionEdit>,
    removed_indexes: RemovedComponents<'w, 's, TerrainWaterMembershipIndex>,
    removed_source: RemovedComponents<'w, 's, TerrainWaterRegionSource>,
    removed_retirement: RemovedComponents<'w, 's, TerrainWaterRegionRetired>,
    resumed: RemovedComponents<'w, 's, AnimalCrate>,
    script_resumed: RemovedComponents<'w, 's, ScriptedCrating>,
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
            Has<AnimalFloatingFallback>,
        ),
        (Without<Dead>, UncratedWorldEntity),
    >,
) {
    let refresh_all = species_index.is_changed()
        | species_assets.is_changed()
        | changes.terrain_assets.is_changed()
        | (changes.removed_water.read().count() != 0)
        | (changes.removed_indexes.read().count() != 0)
        | (changes.removed_source.read().count() != 0)
        | (changes.removed_retirement.read().count() != 0);
    let Some(species) = species_index.get(&species_assets) else {
        return;
    };
    let mut edited: Vec<(Entity, Rect)> = changes
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
    let mut precise_terrain = EntityHashSet::default();
    let mut replaced_terrain = EntityHashSet::default();
    for change in changes.terrain_water_cells.read() {
        let Some(terrain) = changes.terrain_assets.get(change.terrain) else {
            continue;
        };
        let Some(bounds) = &change.source_bounds else {
            replaced_terrain.insert(change.root);
            continue;
        };
        precise_terrain.insert(change.root);
        let cell = terrain.canonical_terrain_grid().units.cell_size_metres;
        let scale = Vec2::new(cell, -cell);
        edited.extend(bounds.iter().map(|(first, last)| {
            let first = first.saturating_sub(UVec2::ONE).as_vec2() * scale;
            let last = (last.as_vec2() + Vec2::ONE) * scale;
            (change.root, Rect::from_corners(first, last))
        }));
    }
    for (entity, member) in &changes.terrain_refresh {
        let root = member.map_or(entity, |member| member.root);
        if !precise_terrain.contains(&root) {
            replaced_terrain.insert(root);
        }
    }
    let mut cell_changes =
        bevy::ecs::entity::EntityHashMap::<bevy::platform::collections::HashSet<IVec2>>::default();
    for change in changes.habitat_cells.read() {
        cell_changes
            .entry(change.root)
            .or_default()
            .extend(change.cells.iter().copied());
    }
    let mut precise_tanks = EntityHashSet::default();
    for change in changes.tank_cells.read() {
        precise_tanks.insert(change.tank);
        cell_changes
            .entry(change.root)
            .or_default()
            .extend(change.cells.iter().copied());
    }
    let refresh_all = refresh_all
        | changes
            .removed_tanks
            .read()
            .any(|entity| !precise_tanks.contains(&entity));
    let changed_tanks: EntityHashSet = changes
        .tanks
        .iter()
        .filter_map(|(entity, member)| (!precise_tanks.contains(&entity)).then_some(member.root))
        .collect();
    let resumed: EntityHashSet = changes
        .resumed
        .read()
        .chain(changes.script_resumed.read())
        .chain(changes.revived.read())
        .chain(changes.changed_modes.read())
        .chain(changes.enabled.read())
        .collect();
    for (entity, member, variant, mut transform, current_mode, contact, was_floating) in
        &mut animals
    {
        let position = transform.translation.xz();
        if !refresh_all
            && !replaced_terrain.contains(&member.root)
            && !member.is_changed()
            && !variant.is_changed()
            && !transform.is_changed()
            && !current_mode.as_ref().is_some_and(DetectChanges::is_changed)
            && contact.is_some()
            && !resumed.contains(&entity)
            && !cell_changes.get(&member.root).is_some_and(|changed| {
                changes
                    .grids
                    .get(member.root)
                    .ok()
                    .and_then(|grid| {
                        convert_finite_world_position_to_topology_cell(
                            position - grid.origin,
                            grid.spacing_m,
                        )
                    })
                    .is_some_and(|coordinate| changed.contains(&coordinate))
            })
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
        let Some(definition) = species.find_variant(variant.0) else {
            continue;
        };
        let policy = definition.surface_water_locomotion.as_ref();
        let current = current_mode.as_deref().copied().unwrap_or_default();
        let floating_surface = surface.filter(|&height| {
            current == LocomotionMode::Ground
                && policy.is_none()
                && (transform.translation.y < height
                    || (was_floating && transform.translation.y == height))
        });
        match (was_floating, floating_surface) {
            (false, Some(_)) => {
                commands.entity(entity).insert(AnimalFloatingFallback);
            }
            (true, None) => {
                commands.entity(entity).remove::<AnimalFloatingFallback>();
            }
            _ => {}
        }
        if let Some(height) = floating_surface {
            if transform.translation.y != height {
                transform.translation.y = height;
            }
        }
        let Some(policy) = policy else {
            continue;
        };
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
