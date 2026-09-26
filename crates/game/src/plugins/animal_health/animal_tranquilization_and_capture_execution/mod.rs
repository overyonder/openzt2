use bevy::prelude::*;

use crate::assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use crate::assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use crate::plugins::animal_lifecycle::types::Animal;
use crate::plugins::staff::staff_employment_types::Staff;

use super::{
    tranquilizer_eligibility_calculation::authored_tranquilizer_allows_animal_state,
    types::{
        AnimalCaptured, CaptureAnimalRequest, Dead, Escaped, Rampaging, RecoveringFromTranquilizer,
        TranquilizeRequest, Tranquilized, TranquilizerTool,
    },
};

pub(super) fn validate_tranquilizer_requests_and_apply_timed_tranquilization(
    mut commands: Commands,
    mut tranquilizer_requests: MessageReader<TranquilizeRequest>,
    world_definition_assets: Res<Assets<WorldDefinitionAsset>>,
    active_world_definitions: Res<WorldDefinitions>,
    animals: Query<
        (
            &GlobalTransform,
            Option<&Escaped>,
            Option<&Rampaging>,
            Option<&Tranquilized>,
        ),
        (With<Animal>, Without<Dead>),
    >,
    tranquilizer_sources: Query<&GlobalTransform, Or<(With<Staff>, With<TranquilizerTool>)>>,
) {
    let Some(world_definitions) = active_world_definitions.get(&world_definition_assets) else {
        return;
    };
    let Some(tranquilizer_mode) = world_definitions.tranquilizer_mode() else {
        return;
    };
    for request in tranquilizer_requests.read() {
        let Ok(source_transform) = tranquilizer_sources.get(request.source) else {
            continue;
        };
        let Ok((animal_transform, escaped, rampaging, existing_tranquilization)) =
            animals.get(request.animal)
        else {
            continue;
        };
        let Some(tranquilizer_definition) =
            world_definitions.find_tranquilizer(request.tranquilizer)
        else {
            continue;
        };
        let maximum_range_m = tranquilizer_mode.range_cm as f32 / 100.0;
        if !authored_tranquilizer_allows_animal_state(
            tranquilizer_definition.eligible_states,
            escaped.is_some(),
            rampaging.is_some(),
        ) || source_transform
            .translation()
            .distance_squared(animal_transform.translation())
            > maximum_range_m * maximum_range_m
        {
            continue;
        }
        let remaining_ticks =
            existing_tranquilization.map_or(tranquilizer_definition.duration_ticks, |current| {
                current
                    .remaining_ticks
                    .max(tranquilizer_definition.duration_ticks)
            });
        commands.entity(request.animal).insert(Tranquilized {
            remaining_ticks,
            recovery_ticks: tranquilizer_definition.recovery_ticks,
        });
    }
}

pub(super) fn validate_staff_capture_requests_and_clear_animal_emergency_state(
    mut commands: Commands,
    mut capture_requests: MessageReader<CaptureAnimalRequest>,
    animals: Query<
        (),
        (
            With<Animal>,
            With<Escaped>,
            With<Tranquilized>,
            Without<Dead>,
        ),
    >,
    staff: Query<(), With<Staff>>,
    mut animal_captured_messages: MessageWriter<AnimalCaptured>,
) {
    for request in capture_requests.read() {
        if animals.get(request.animal).is_err() || staff.get(request.staff).is_err() {
            continue;
        }
        commands
            .entity(request.animal)
            .remove::<(Escaped, Rampaging, Tranquilized)>();
        animal_captured_messages.write(AnimalCaptured {
            animal: request.animal,
            staff: request.staff,
        });
    }
}

pub(super) fn advance_animal_tranquilization_and_begin_recovery_period(
    mut commands: Commands,
    mut tranquilized_animals: Query<(Entity, &mut Tranquilized), (With<Animal>, Without<Dead>)>,
) {
    for (animal, mut tranquilized) in &mut tranquilized_animals {
        tranquilized.remaining_ticks = tranquilized.remaining_ticks.saturating_sub(1);
        if tranquilized.remaining_ticks == 0 {
            let recovery_ticks = tranquilized.recovery_ticks;
            let mut animal_commands = commands.entity(animal);
            animal_commands.remove::<Tranquilized>();
            if recovery_ticks != 0 {
                animal_commands.insert(RecoveringFromTranquilizer {
                    remaining_ticks: recovery_ticks,
                });
            }
        }
    }
}

pub(super) fn advance_and_complete_animal_tranquilizer_recovery_periods(
    mut commands: Commands,
    mut recovering_animals: Query<
        (Entity, &mut RecoveringFromTranquilizer),
        (With<Animal>, Without<Dead>),
    >,
) {
    for (animal, mut recovery) in &mut recovering_animals {
        recovery.remaining_ticks = recovery.remaining_ticks.saturating_sub(1);
        if recovery.remaining_ticks == 0 {
            commands
                .entity(animal)
                .remove::<RecoveringFromTranquilizer>();
        }
    }
}
