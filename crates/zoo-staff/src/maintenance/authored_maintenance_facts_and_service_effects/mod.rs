use bevy::prelude::*;
use openzt2_game_data::world_definitions::facilities_and_maintenance::{
    FacilityServiceMaintenanceEffect, MaintenanceDefinition,
};
use openzt2_game_data::AssetId;

use openzt2_game_assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use openzt2_zoo_state::economy::service_types::ServiceCompleted;
use openzt2_world_state::world_spawn::persistent_id_types::PersistentIdAllocator;
use openzt2_world_state::world_spawn::world_membership_types::DefinitionId;
use openzt2_world_state::world_spawn::world_membership_types::WorldMember;

use super::maintenance_types::{FacilityContainedWaste, LooseLitterWaste};

#[derive(Component)]
pub(super) struct ProjectedMaintenanceContainer;

pub(super) fn project_authored_maintenance_capacity(
    mut commands: Commands,
    world_definition_assets: Res<Assets<WorldDefinitionAsset>>,
    active_world_definitions: Res<WorldDefinitions>,
    (mut removed, changed): (
        RemovedComponents<DefinitionId>,
        Query<Entity, Changed<DefinitionId>>,
    ),
    mut objects: Query<(
        Entity,
        &DefinitionId,
        Option<&mut FacilityContainedWaste>,
        Has<ProjectedMaintenanceContainer>,
    )>,
    lost_definitions: Query<(), (With<ProjectedMaintenanceContainer>, Without<DefinitionId>)>,
) {
    for entity in removed
        .read()
        .filter(|&entity| lost_definitions.contains(entity))
    {
        commands
            .entity(entity)
            .remove::<(FacilityContainedWaste, ProjectedMaintenanceContainer)>();
    }
    let refresh_all = active_world_definitions.is_changed() || world_definition_assets.is_changed();
    let Some(world_definitions) = active_world_definitions.get(&world_definition_assets) else {
        return;
    };
    // An unavailable higher-precedence document must not initialize from an
    // older policy. Unresolved objects retry when the asset set is available.
    if world_definitions.has_pending_assets() {
        return;
    }
    let candidates: Vec<Entity> = if refresh_all {
        objects.iter().map(|(entity, ..)| entity).collect()
    } else {
        changed.iter().collect()
    };
    for candidate in candidates {
        let Ok((entity, definition, current, owned)) = objects.get_mut(candidate) else {
            continue;
        };
        let Some(policy) = world_definitions
            .find_maintenance_by_object(definition.0)
            .filter(|policy| policy.waste_capacity_units > 0)
        else {
            if owned {
                commands
                    .entity(entity)
                    .remove::<(FacilityContainedWaste, ProjectedMaintenanceContainer)>();
            }
            continue;
        };
        let capacity = policy.waste_capacity_units;
        if let Some(mut current) = current {
            let units = current.contained_waste_units.min(capacity);
            current
                .reborrow()
                .map_unchanged(|waste| &mut waste.contained_waste_capacity_units)
                .set_if_neq(capacity);
            current
                .map_unchanged(|waste| &mut waste.contained_waste_units)
                .set_if_neq(units);
        } else {
            commands.entity(entity).insert(FacilityContainedWaste {
                contained_waste_units: policy.initial_waste_units.min(capacity),
                contained_waste_capacity_units: capacity,
            });
        }
        if !owned {
            commands
                .entity(entity)
                .insert(ProjectedMaintenanceContainer);
        }
    }
}

pub(super) fn apply_completed_service_maintenance_effects_to_facilities(
    mut commands: Commands,
    mut completed_services: MessageReader<ServiceCompleted>,
    world_definition_assets: Res<Assets<WorldDefinitionAsset>>,
    active_world_definitions: Res<WorldDefinitions>,
    mut persistent_identifiers: Query<&mut PersistentIdAllocator>,
    mut facilities: Query<(
        &DefinitionId,
        &Transform,
        &WorldMember,
        Option<&mut FacilityContainedWaste>,
    )>,
) {
    let Some(world_definitions) = active_world_definitions.get(&world_definition_assets) else {
        return;
    };
    for completed_service in completed_services.read() {
        let Ok((definition_id, transform, world_member, waste_container)) =
            facilities.get_mut(completed_service.facility)
        else {
            continue;
        };
        let Some(maintenance_definition) =
            world_definitions.find_maintenance_by_object(definition_id.0)
        else {
            continue;
        };
        let Some(service_effect) =
            find_service_maintenance_effect(maintenance_definition, completed_service.service)
        else {
            continue;
        };
        if service_effect.contained_waste_units != 0 && waste_container.is_none() {
            continue;
        }

        let spawned_litter = if service_effect.loose_litter_units == 0 {
            None
        } else {
            let Some(litter_definition) = &maintenance_definition.litter_definition else {
                continue;
            };
            let Some(persistent_identifier) = persistent_identifiers
                .get_mut(world_member.root)
                .ok()
                .and_then(|mut allocator| allocator.allocate().ok())
            else {
                continue;
            };
            let litter_local_offset = Vec3::new(
                f32::from(maintenance_definition.litter_local_offset_cm[0]),
                f32::from(maintenance_definition.litter_local_offset_cm[1]),
                f32::from(maintenance_definition.litter_local_offset_cm[2]),
            ) / 100.0;
            Some((
                persistent_identifier,
                DefinitionId(AssetId(litter_definition.0)),
                Transform::from_translation(transform.transform_point(litter_local_offset)),
                *world_member,
                LooseLitterWaste {
                    uncontained_waste_units: service_effect.loose_litter_units,
                },
            ))
        };

        if let Some(waste_container) = waste_container {
            let units = waste_container
                .contained_waste_units
                .saturating_add(service_effect.contained_waste_units)
                .min(waste_container.contained_waste_capacity_units);
            waste_container
                .map_unchanged(|waste| &mut waste.contained_waste_units)
                .set_if_neq(units);
        }
        if let Some(spawned_litter) = spawned_litter {
            commands.spawn(spawned_litter);
        }
    }
}

fn find_service_maintenance_effect(
    maintenance_definition: &MaintenanceDefinition,
    service_asset_identifier: AssetId,
) -> Option<&FacilityServiceMaintenanceEffect> {
    maintenance_definition
        .service_effects
        .iter()
        .find(|service_effect| service_effect.service == service_asset_identifier)
}
