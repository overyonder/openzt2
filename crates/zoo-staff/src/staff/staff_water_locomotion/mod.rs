//! Staff swim where water is deep enough under them, as their authored swim
//! component decides, and float at its surface or at the depth a submerged
//! behavior holds them at.

use bevy::prelude::*;

use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use openzt2_game_assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use openzt2_zoo_state::locomotion::locomotion_types::{LocomotionMode, SwimLayer};
use openzt2_zoo_state::staff::staff_employment_types::{Staff, StaffRole, StaffSubmergedDepth};
use openzt2_zoo_state::terrain::dry_surface_eligibility::DrySurfaceEligibility;
use openzt2_world_state::world_spawn::persistent_id_types::PersistentId;
use openzt2_zoo_state::staff::staff_presentation_variant_selection::{
    dress_staff, staff_presentation_variant, StaffDress,
};

/// Enters swimming in water deeper than the authored entry depth and leaves
/// it where water is shallower than the authored exit depth, then keeps a
/// swimmer at the surface, or at its held depth, after motion has moved it.
pub(crate) fn project_staff_water_locomotion(
    mut commands: Commands,
    definitions: Res<Assets<WorldDefinitionAsset>>,
    active_definitions: Res<WorldDefinitions>,
    surfaces: DrySurfaceEligibility,
    mut staff: Query<
        (
            Entity,
            &StaffRole,
            &mut Transform,
            Option<&LocomotionMode>,
            Option<&StaffSubmergedDepth>,
        ),
        (
            With<Staff>,
            Without<openzt2_zoo_state::staff::staff_employment_types::StaffClimbingTankGate>,
        ),
    >,
) {
    let Some(catalogue) = active_definitions.get(&definitions) else {
        return;
    };
    for (employee, role, mut transform, mode, held_depth) in &mut staff {
        let Some(depths) = catalogue.find_staff(role.0).and_then(|role| role.swimming) else {
            continue;
        };
        let position = transform.translation.xz();
        let surface = surfaces.water_surface_height(position);
        let water_depth = surface
            .zip(surfaces.ground_height(position))
            .map_or(0.0, |(surface, ground)| surface - ground);
        let swimming = mode == Some(&LocomotionMode::Swim);
        let swims = if swimming {
            water_depth >= depths.exit_depth_m
        } else {
            water_depth > depths.entry_depth_m
        };
        if swims != swimming || mode.is_none() {
            if swims {
                commands
                    .entity(employee)
                    .insert((LocomotionMode::Swim, SwimLayer::Surface));
            } else {
                commands
                    .entity(employee)
                    .insert(LocomotionMode::Ground)
                    .remove::<(SwimLayer, StaffSubmergedDepth)>();
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
        commands.entity(employee).insert(layer);
        transform.translation.y = surface - held;
    }
}

/// Puts a keeper in its wetsuit when it starts swimming and back in its own
/// look when it leaves the water, as `ZTWetsuitComponent` swaps them.
pub(crate) fn dress_staff_for_water(
    mut commands: Commands,
    definitions: Res<Assets<WorldDefinitionAsset>>,
    active_definitions: Res<WorldDefinitions>,
    staff: Query<
        (
            Entity,
            &StaffRole,
            &PersistentId,
            &LocomotionMode,
            &StaffDress,
        ),
        (With<Staff>, Changed<LocomotionMode>),
    >,
) {
    let Some(catalogue) = active_definitions.get(&definitions) else {
        return;
    };
    for (employee, role, id, mode, dress) in &staff {
        let wet = mode == &LocomotionMode::Swim;
        if dress.wet == wet {
            continue;
        }
        let Some((variant, _)) = catalogue
            .find_staff(role.0)
            .and_then(|role| staff_presentation_variant(role, id.0, catalogue))
        else {
            continue;
        };
        if variant.wetsuit.is_none() {
            continue;
        }
        if let Some(head) = dress.head {
            commands.entity(head).try_despawn();
        }
        dress_staff(&mut commands, employee, variant, id.0, catalogue, wet);
    }
}
