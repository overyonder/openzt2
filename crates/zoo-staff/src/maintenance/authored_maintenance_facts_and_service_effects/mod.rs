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

use super::maintenance_types::{
    AuthoredMaintenanceFactsResolutionMarker, FacilityContainedWaste,
    IncrementalZooCleanlinessTotals, LooseLitterWaste,
};

pub(super) fn initialize_authored_maintenance_facts_for_world_objects(
    mut commands: Commands,
    world_definition_assets: Res<Assets<WorldDefinitionAsset>>,
    active_world_definitions: Res<WorldDefinitions>,
    unresolved_world_objects: Query<
        (Entity, &DefinitionId),
        Without<AuthoredMaintenanceFactsResolutionMarker>,
    >,
) {
    let Some(world_definitions) = active_world_definitions.get(&world_definition_assets) else {
        return;
    };
    // An unavailable higher-precedence document must not initialize from an
    // older policy. Unresolved objects retry when the asset set is available.
    if world_definitions.has_pending_assets() {
        return;
    }
    for (world_object_entity, definition_id) in &unresolved_world_objects {
        let Some(maintenance_definition) =
            world_definitions.find_maintenance_by_object(definition_id.0)
        else {
            continue;
        };
        let mut world_object = commands.entity(world_object_entity);
        if maintenance_definition.waste_capacity_units != 0 {
            world_object.insert_if_new(FacilityContainedWaste {
                contained_waste_units: maintenance_definition
                    .initial_waste_units
                    .min(maintenance_definition.waste_capacity_units),
                contained_waste_capacity_units: maintenance_definition.waste_capacity_units,
            });
        }
        world_object.insert(AuthoredMaintenanceFactsResolutionMarker);
    }
}

pub(super) fn apply_completed_service_maintenance_effects_to_facilities(
    mut commands: Commands,
    mut completed_services: MessageReader<ServiceCompleted>,
    world_definition_assets: Res<Assets<WorldDefinitionAsset>>,
    active_world_definitions: Res<WorldDefinitions>,
    mut persistent_identifier_allocator: ResMut<PersistentIdAllocator>,
    mut facilities: Query<(
        &DefinitionId,
        &Transform,
        &WorldMember,
        Option<&mut FacilityContainedWaste>,
    )>,
    mut cleanliness_totals: ResMut<IncrementalZooCleanlinessTotals>,
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
            let Ok(persistent_identifier) =
                persistent_identifier_allocator.allocate(world_member.root)
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

        if let Some(mut waste_container) = waste_container {
            let previous_waste_container = *waste_container;
            waste_container.contained_waste_units = waste_container
                .contained_waste_units
                .saturating_add(service_effect.contained_waste_units)
                .min(waste_container.contained_waste_capacity_units);
            cleanliness_totals
                .replace_facility_contained_waste(previous_waste_container, *waste_container);
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
