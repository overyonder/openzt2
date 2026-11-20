use bevy::{ecs::relationship::RelationshipTarget, prelude::*};

use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitionsView;
use openzt2_world_state::world_spawn::persistent_id_assignment::AssignPersistentId;
use openzt2_world_state::world_spawn::world_membership_types::WorldMember;

use super::{
    show_editor_interaction_types::{
        AddShowConfirmationPending, DeleteShowConfirmationPending, EditedShow, ShowEditing,
    },
    show_schedule_types::{
        ScheduledShowBreakRow, ScheduledShowPerformancePlan, ScheduledShowRow,
        ScheduledShowRowActivationState, SelectedShowScheduleRow, ShowScheduleRowOrder,
        ShowStageRows,
    },
    show_stage_types::ShowStage,
    show_timing_operations::convert_authored_nanoseconds_to_fixed_simulation_ticks,
};

pub(super) type AuthoredShowScheduleRowQuery<'world, 'state> = Query<
    'world,
    'state,
    (
        &'static ScheduledShowRow,
        &'static mut ShowScheduleRowOrder,
        Has<ScheduledShowPerformancePlan>,
    ),
>;

pub(super) fn create_scheduled_show_row_after_authored_confirmation(
    controller: Entity,
    confirmations: &Query<&AddShowConfirmationPending>,
    stages: &Query<&ShowStage>,
    definitions: WorldDefinitionsView<'_>,
    commands: &mut Commands,
) {
    let Ok(confirmation) = confirmations.get(controller) else {
        return;
    };
    let stage = confirmation.stage;
    let Some(definition) = stages
        .get(stage)
        .ok()
        .and_then(|stage| definitions.find_show_stage(stage.show_stage_definition))
    else {
        return;
    };
    let capacity = definition.schedule_slots;
    commands.queue(move |world: &mut World| {
        if world
            .get::<AddShowConfirmationPending>(controller)
            .is_none_or(|confirmation| confirmation.stage != stage)
            || world.get::<ShowStage>(stage).is_none()
        {
            return;
        }
        let row = spawn_stage_schedule_row(
            world,
            stage,
            (
                ScheduledShowPerformancePlan {
                    trick_performance_capacity: capacity,
                },
                ScheduledShowRowActivationState::Enabled,
            ),
        );
        world
            .entity_mut(controller)
            .remove::<AddShowConfirmationPending>()
            .insert((SelectedShowScheduleRow(row), EditedShow(row), ShowEditing));
    });
}

pub(super) fn create_show_schedule_break_row_for_selected_stage(
    stage: Entity,
    definitions: WorldDefinitionsView<'_>,
    commands: &mut Commands,
) {
    let duration_ticks = definitions.show_scheduling().map_or(1, |schedule| {
        convert_authored_nanoseconds_to_fixed_simulation_ticks(
            schedule.between_show_ns,
            definitions,
        )
        .max(1)
    });
    commands.queue(move |world: &mut World| {
        if world.get::<ShowStage>(stage).is_some() {
            spawn_stage_schedule_row(world, stage, ScheduledShowBreakRow { duration_ticks });
        }
    });
}

fn spawn_stage_schedule_row(world: &mut World, stage: Entity, row: impl Bundle) -> Entity {
    let order = world
        .get::<ShowStageRows>(stage)
        .into_iter()
        .flat_map(RelationshipTarget::iter)
        .filter_map(|row| world.get::<ShowScheduleRowOrder>(row))
        .map(|order| order.0)
        .max()
        .map_or(0, |order| order.saturating_add(1));
    let entity = world
        .spawn((ScheduledShowRow { stage }, ShowScheduleRowOrder(order), row))
        .id();
    if let Some(member) = world.get::<WorldMember>(stage).copied() {
        world.entity_mut(entity).insert(member);
        world.write_message(AssignPersistentId {
            entity,
            root: member.root,
        });
    }
    entity
}

pub(super) fn delete_confirmed_show_schedule_row_and_close_editor_selection(
    controller: Entity,
    confirmations: &Query<&DeleteShowConfirmationPending>,
    rows: &mut AuthoredShowScheduleRowQuery<'_, '_>,
    stage_rows: &Query<&ShowStageRows>,
    commands: &mut Commands,
) {
    let Ok(confirmation) = confirmations.get(controller) else {
        return;
    };
    if let Ok((owner, order, _)) = rows.get(confirmation.entry) {
        let previous = order.0;
        if let Ok(stage_rows) = stage_rows.get(owner.stage) {
            for row in stage_rows.iter() {
                if let Ok((_, mut order, _)) = rows.get_mut(row) {
                    if order.0 > previous {
                        order.0 -= 1;
                    }
                }
            }
        }
        commands.entity(confirmation.entry).try_despawn();
    }
    commands.entity(controller).try_remove::<(
        SelectedShowScheduleRow,
        DeleteShowConfirmationPending,
        EditedShow,
        ShowEditing,
    )>();
}

pub(super) fn move_show_schedule_row_by_one_position(
    selected: Entity,
    direction: i8,
    rows: &mut AuthoredShowScheduleRowQuery<'_, '_>,
    stage_rows: &Query<&ShowStageRows>,
) {
    let Ok((owner, order, _)) = rows.get(selected) else {
        return;
    };
    let previous = order.0;
    let Some(target) = previous.checked_add_signed(i16::from(direction)) else {
        return;
    };
    let Ok(stage_rows) = stage_rows.get(owner.stage) else {
        return;
    };
    let Some(other) = stage_rows
        .iter()
        .find(|&row| rows.get(row).is_ok_and(|(_, order, _)| order.0 == target))
    else {
        return;
    };
    if let Ok([(_, mut selected, _), (_, mut other, _)]) = rows.get_many_mut([selected, other]) {
        selected.set_if_neq(ShowScheduleRowOrder(target));
        other.set_if_neq(ShowScheduleRowOrder(previous));
    }
}
