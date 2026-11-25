//! Staff swim where water is deep enough under them, as their authored swim
//! component decides, and float at its surface or at the depth a submerged
//! behavior holds them at.

use bevy::{
    ecs::{entity::EntityHashSet, entity_disabling::Disabled, system::SystemParam},
    prelude::*,
};
use openzt2_game_assets::terrain::terrain_asset_types_and_borrowing_queries::TerrainAsset;
use openzt2_world_state::world_spawn::world_membership_types::WorldMember;

use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use openzt2_game_assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use openzt2_zoo_state::locomotion::locomotion_types::{LocomotionMode, SwimLayer};
use openzt2_zoo_state::staff::staff_employment_types::{Staff, StaffClimbingTankGate, StaffRole, StaffSubmergedDepth};
use openzt2_zoo_state::terrain::dry_surface_eligibility::DrySurfaceEligibility;
use openzt2_zoo_state::{
    aquatic::aquatic_simulation_types::TankGeometry,
    habitat::habitat_types::HabitatIndex,
    terrain::{
        terrain_chunk_types::{TerrainChunk, TerrainIndex},
        terrain_navigation_change_types::{TerrainNavigationChanged, TerrainNavigationRestored},
        terrain_water_presentation_types::{TerrainWaterMembershipIndex, TerrainWaterRegionEdit, TerrainWaterRegionRetired, TerrainWaterRegionSource},
    },
};

#[derive(SystemParam)]
pub(super) struct StaffWaterChanges<'w, 's> {
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
    edits: MessageReader<'w, 's, TerrainNavigationChanged>,
    restores: MessageReader<'w, 's, TerrainNavigationRestored>,
    chunks: Query<'w, 's, (&'static TerrainChunk, &'static WorldMember)>,
    removed_tanks: RemovedComponents<'w, 's, TankGeometry>,
    removed_water: RemovedComponents<'w, 's, TerrainWaterRegionEdit>,
    removed_indexes: RemovedComponents<'w, 's, TerrainWaterMembershipIndex>,
    removed_source: RemovedComponents<'w, 's, TerrainWaterRegionSource>,
    removed_retirement: RemovedComponents<'w, 's, TerrainWaterRegionRetired>,
    unmounted: RemovedComponents<'w, 's, StaffClimbingTankGate>,
    released_depth: RemovedComponents<'w, 's, StaffSubmergedDepth>,
    enabled: RemovedComponents<'w, 's, Disabled>,
}

/// Enters swimming in water deeper than the authored entry depth and leaves
/// it where water is shallower than the authored exit depth, then keeps a
/// swimmer at the surface, or at its held depth, after motion has moved it.
#[allow(clippy::type_complexity)]
pub(super) fn project_staff_water_locomotion(
    mut commands: Commands,
    definitions: Res<Assets<WorldDefinitionAsset>>,
    active_definitions: Res<WorldDefinitions>,
    surfaces: DrySurfaceEligibility,
    mut changes: StaffWaterChanges,
    mut staff: Query<
        (
            Entity,
            Ref<StaffRole>,
            Ref<WorldMember>,
            &mut Transform,
            Option<&mut LocomotionMode>,
            Option<&mut SwimLayer>,
            Option<Ref<StaffSubmergedDepth>>,
        ),
        (With<Staff>, Without<StaffClimbingTankGate>),
    >,
) {
    let refresh_all = definitions.is_changed()
        | active_definitions.is_changed()
        | changes.terrain_assets.is_changed()
        | !changes.terrain_refresh.is_empty()
        | (changes.removed_tanks.read().count() != 0)
        | (changes.removed_water.read().count() != 0)
        | (changes.removed_indexes.read().count() != 0)
        | (changes.removed_source.read().count() != 0)
        | (changes.removed_retirement.read().count() != 0);
    let Some(catalogue) = active_definitions.get(&definitions) else {
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
            let first = chunk.origin + min.saturating_sub(UVec2::ONE).as_vec2() * chunk.spacing_m;
            let last = chunk.origin + (max.as_vec2() + Vec2::ONE) * chunk.spacing_m;
            Some((member.root, Rect::from_corners(first, last)))
        })
        .collect();
    let changed_tanks: EntityHashSet = changes.tanks.iter().map(|member| member.root).collect();
    let resumed: EntityHashSet = changes
        .unmounted
        .read()
        .chain(changes.released_depth.read())
        .chain(changes.enabled.read())
        .collect();
    for (employee, role, member, transform, mode, swim_layer, held_depth) in &mut staff {
        let position = transform.translation.xz();
        if !refresh_all
            && !role.is_changed()
            && !member.is_changed()
            && !transform.is_changed()
            && mode.as_ref().is_some_and(|mode| !mode.is_changed())
            && !held_depth.as_ref().is_some_and(DetectChanges::is_changed)
            && !resumed.contains(&employee)
            && !changes.habitats.contains(member.root)
            && !changed_tanks.contains(&member.root)
            && !edited
                .iter()
                .any(|(root, bounds)| *root == member.root && bounds.contains(position))
        {
            continue;
        }
        let Some(depths) = catalogue.find_staff(role.0).and_then(|role| role.swimming) else {
            continue;
        };
        let surface = surfaces.water_surface_height(member.root, position);
        let water_depth = surface
            .zip(surfaces.ground_height(member.root, position))
            .map_or(0.0, |(surface, ground)| surface - ground);
        let swimming = mode.as_deref() == Some(&LocomotionMode::Swim);
        let swims = if swimming {
            water_depth >= depths.exit_depth_m
        } else {
            water_depth > depths.entry_depth_m
        };
        if swims != swimming || mode.is_none() {
            let next = if swims {
                LocomotionMode::Swim
            } else {
                LocomotionMode::Ground
            };
            if let Some(mut mode) = mode {
                mode.set_if_neq(next);
            } else {
                commands.entity(employee).insert(next);
            }
            if !swims {
                commands
                    .entity(employee)
                    .remove::<(SwimLayer, StaffSubmergedDepth)>();
            } else if surface.is_none() && swim_layer.is_none() {
                commands.entity(employee).insert(SwimLayer::Surface);
            }
        }
        let Some(surface) = surface.filter(|_| swims) else {
            continue;
        };
        let held = held_depth.map_or(0.0, |depth| depth.0.clamp(0.0, water_depth.max(0.0)));
        let layer = if held > 0.0 {
            SwimLayer::Submerged
        } else {
            SwimLayer::Surface
        };
        if let Some(mut swim_layer) = swim_layer {
            swim_layer.set_if_neq(layer);
        } else {
            commands.entity(employee).insert(layer);
        }
        transform
            .map_unchanged(|transform| &mut transform.translation.y)
            .set_if_neq(surface - held);
    }
}
