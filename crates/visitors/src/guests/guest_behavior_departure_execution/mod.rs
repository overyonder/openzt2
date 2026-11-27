use bevy::prelude::*;
use openzt2_world_state::world_spawn::world_membership_types::WorldMember;
use openzt2_zoo_state::{
    behavior_task_execution_types::{
        running_actions::{RunBehaviorTermination, RunningBehaviorInstruction, WhenBehaviorAction},
        BehaviorTaskExecutionState,
    },
    guests::guest_simulation_types::{
        DeparturePending, Guest, GuestDeparted, GuestEducation, GuestSatisfaction,
    },
};

pub(super) fn remove_guest_after_authored_departure(
    activation: On<WhenBehaviorAction<RunBehaviorTermination>>,
    guests: Query<
        (
            &BehaviorTaskExecutionState,
            &RunningBehaviorInstruction,
            &RunBehaviorTermination,
            &WorldMember,
            &GuestSatisfaction,
            &GuestEducation,
        ),
        (With<Guest>, Without<DeparturePending>),
    >,
    mut departed: MessageWriter<GuestDeparted>,
    mut commands: Commands,
) {
    let guest = activation.entity;
    let Ok((task, instruction, termination, world, satisfaction, education)) = guests.get(guest)
    else {
        return;
    };
    if !instruction.is_current(task) || !termination.0.kill_subject {
        return;
    }
    departed.write(GuestDeparted {
        root: world.root,
        guest,
        satisfaction_permille: satisfaction.0,
        education_permille: education.0,
    });
    commands.entity(guest).insert_if_new(DeparturePending);
}

pub(super) fn departure_capacity_allows_score(
    admissions_open: bool,
    departure_points_q16: i32,
    observed: usize,
    required: usize,
    available_tour: Option<(f32, f32)>,
) -> bool {
    !admissions_open
        || departure_points_q16 <= 0
        || (observed >= required && available_tour.is_none_or(|(need, cutoff)| need <= cutoff))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn open_gate_applies_view_capacity_only_while_departure_points_are_positive() {
        assert!(!departure_capacity_allows_score(true, 1, 0, 2, None));
        assert!(departure_capacity_allows_score(true, 0, 0, 2, None));
        assert!(departure_capacity_allows_score(false, 1, 0, 2, None));
    }

    #[test]
    fn available_tour_only_blocks_departure_above_the_tour_cutoff() {
        assert!(departure_capacity_allows_score(
            true,
            1,
            2,
            2,
            Some((50.0, 50.0))
        ));
        assert!(!departure_capacity_allows_score(
            true,
            1,
            2,
            2,
            Some((50.01, 50.0))
        ));
    }
}

#[derive(Component, Default)]
pub(super) struct GuestDepartureFailureDelta(f32);

use openzt2_game_assets::world_definitions::{
    world_definition_asset_set_state_and_borrowing_queries::{
        WorldDefinitions, WorldDefinitionsView,
    },
    world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset,
};
use openzt2_game_data::AssetId;
use openzt2_zoo_state::{
    authored_float_attributes::AuthoredFloatAttributes,
    behavior_task_execution_types::BehaviorTaskFailed,
    economy::facility_economy_types::Wallet,
    guests::guest_simulation_types::{
        GuestArchetype, GuestDeparturePoints, GuestNeedAdjustmentDue,
    },
};

fn guest_number(
    definitions: WorldDefinitionsView<'_>,
    archetype: AssetId,
    attributes: Option<&AuthoredFloatAttributes>,
    property: &str,
) -> Option<f32> {
    let key = AssetId::from_key(property);
    attributes
        .and_then(|attributes| attributes.get(key))
        .or_else(|| {
            let object = definitions.find_guest(archetype)?.object;
            definitions
                .find_object(object)?
                .authored_runtime
                .numeric_behavior_properties
                .iter()
                .find_map(|&(property, value)| (property == key).then_some(value))
        })
}

pub(super) fn update_due_guest_departure_pressure(
    mut updates: MessageReader<GuestNeedAdjustmentDue>,
    active: Res<WorldDefinitions>,
    assets: Res<Assets<WorldDefinitionAsset>>,
    mut guests: Query<(
        &GuestArchetype,
        Option<&AuthoredFloatAttributes>,
        &GuestSatisfaction,
        &Wallet,
        &mut GuestDeparturePoints,
    )>,
) {
    let Some(definitions) = active.get(&assets) else {
        updates.clear();
        return;
    };
    for update in updates.read() {
        let Ok((archetype, attributes, happiness, bank, points)) = guests.get_mut(update.0) else {
            continue;
        };
        let property = |key| guest_number(definitions, archetype.0, attributes, key);
        let mut delta = 0.0;
        if property("f_lowhappiness")
            .is_some_and(|cutoff| happiness.source_q16() as f32 > cutoff * 65_536.0)
        {
            delta += property("f_lowhappydepartpts").unwrap_or_default();
        }
        if !bank.0.is_positive() {
            delta += property("f_lowcashdepartpts").unwrap_or_default();
        }
        add_departure_delta(points, delta);
    }
}

pub(super) fn apply_guest_task_failure_departure_delta(
    mut failures: MessageReader<BehaviorTaskFailed>,
    active: Res<WorldDefinitions>,
    assets: Res<Assets<WorldDefinitionAsset>>,
    mut guests: Query<
        (
            &GuestArchetype,
            Option<&AuthoredFloatAttributes>,
            &mut GuestDeparturePoints,
            &mut GuestDepartureFailureDelta,
        ),
        With<Guest>,
    >,
) {
    let Some(definitions) = active.get(&assets) else {
        failures.clear();
        return;
    };
    for failure in failures.read() {
        let Ok((archetype, attributes, points, mut delta)) = guests.get_mut(failure.actor) else {
            continue;
        };
        let Some(velocity) =
            guest_number(definitions, archetype.0, attributes, "f_failuredepartvel")
        else {
            continue;
        };
        add_departure_delta(points, delta.0);
        delta.0 += velocity;
    }
}

fn add_departure_delta(points: Mut<GuestDeparturePoints>, delta: f32) {
    let next = points
        .0
        .saturating_add((delta * 65_536.0).round() as i32)
        .max(0);
    points
        .map_unchanged(|points| &mut points.0)
        .set_if_neq(next);
}
