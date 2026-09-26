use bevy::prelude::*;
use openzt2_game_data::world_definitions::facilities_and_maintenance::{
    FacilityServiceMaintenanceEffect, MaintenanceDefinition,
};
use openzt2_game_data::AssetId;

use crate::assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use crate::assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use crate::plugins::economy::service_types::ServiceCompleted;
use crate::plugins::simulation_time::simulation_clock_types::ZooDayAdvanced;
use crate::plugins::world_spawn::persistent_id_types::PersistentIdAllocator;
use crate::plugins::world_spawn::world_membership_types::DefinitionId;
use crate::plugins::world_spawn::world_membership_types::WorldMember;

use super::maintenance_types::{
    AuthoredMaintenanceFactsResolutionMarker, FacilityContainedWaste,
    IncrementalZooCleanlinessTotals, LooseLitterWaste, MaintainableObjectConditionPermille,
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
    for (world_object_entity, definition_id) in &unresolved_world_objects {
        let Some(maintenance_definition) =
            world_definitions.find_maintenance_by_object(definition_id.0)
        else {
            commands
                .entity(world_object_entity)
                .insert(AuthoredMaintenanceFactsResolutionMarker);
            continue;
        };
        let initial_condition =
            MaintainableObjectConditionPermille(maintenance_definition.initial_condition_permille);
        if maintenance_definition.waste_capacity_units == 0 {
            commands
                .entity(world_object_entity)
                .insert((initial_condition, AuthoredMaintenanceFactsResolutionMarker));
        } else {
            commands.entity(world_object_entity).insert((
                initial_condition,
                FacilityContainedWaste {
                    contained_waste_units: 0,
                    contained_waste_capacity_units: maintenance_definition.waste_capacity_units,
                },
                AuthoredMaintenanceFactsResolutionMarker,
            ));
        }
    }
}

pub(super) fn deteriorate_maintainable_world_objects_after_zoo_days_advance(
    mut advanced_zoo_days: MessageReader<ZooDayAdvanced>,
    world_definition_assets: Res<Assets<WorldDefinitionAsset>>,
    active_world_definitions: Res<WorldDefinitions>,
    mut maintainable_world_objects: Query<(
        &DefinitionId,
        &mut MaintainableObjectConditionPermille,
    )>,
    mut cleanliness_totals: ResMut<IncrementalZooCleanlinessTotals>,
) {
    let Some(world_definitions) = active_world_definitions.get(&world_definition_assets) else {
        return;
    };
    for advanced_zoo_day in advanced_zoo_days.read() {
        let elapsed_zoo_days = advanced_zoo_day
            .current_day
            .saturating_sub(advanced_zoo_day.previous_day);
        if elapsed_zoo_days == 0 {
            continue;
        }
        for (definition_id, mut maintainable_object_condition) in &mut maintainable_world_objects {
            let Some(maintenance_definition) =
                world_definitions.find_maintenance_by_object(definition_id.0)
            else {
                continue;
            };
            let Some(condition_loss_permille) =
                u32::from(maintenance_definition.deterioration_per_zoo_day_permille)
                    .checked_mul(elapsed_zoo_days)
                    .and_then(|condition_loss| u16::try_from(condition_loss.min(1_000)).ok())
            else {
                continue;
            };
            let previous_condition = *maintainable_object_condition;
            maintainable_object_condition.0 = maintainable_object_condition
                .0
                .saturating_sub(condition_loss_permille);
            cleanliness_totals.replace_maintainable_object_condition(
                previous_condition,
                *maintainable_object_condition,
            );
        }
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
        Option<&mut MaintainableObjectConditionPermille>,
        Option<&mut FacilityContainedWaste>,
    )>,
    mut cleanliness_totals: ResMut<IncrementalZooCleanlinessTotals>,
) {
    let Some(world_definitions) = active_world_definitions.get(&world_definition_assets) else {
        return;
    };
    for completed_service in completed_services.read() {
        let Ok((
            definition_id,
            transform,
            world_member,
            maintainable_object_condition,
            waste_container,
        )) = facilities.get_mut(completed_service.facility)
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
        if maintainable_object_condition.is_none()
            || (service_effect.contained_waste_units != 0 && waste_container.is_none())
        {
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

        if let Some(mut maintainable_object_condition) = maintainable_object_condition {
            let previous_condition = *maintainable_object_condition;
            maintainable_object_condition.0 = maintainable_object_condition
                .0
                .saturating_sub(service_effect.condition_loss_permille);
            cleanliness_totals.replace_maintainable_object_condition(
                previous_condition,
                *maintainable_object_condition,
            );
        }
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
