use bevy::prelude::*;

use crate::assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitionsView;
use crate::plugins::world_spawn::persistent_id_assignment::AssignPersistentId;
use crate::plugins::world_spawn::world_membership_types::WorldMember;

use super::{
    show_editor_interaction_types::{
        AddShowConfirmationPending, DeleteShowConfirmationPending, EditedShow, ShowEditing,
    },
    show_schedule_types::{
        ScheduledShowBreakRow, ScheduledShowPerformancePlan, ScheduledShowRow,
        SelectedShowScheduleRow, ShowScheduleRowOrder,
    },
    show_stage_types::ShowStage,
    show_timing_operations::convert_authored_nanoseconds_to_fixed_simulation_ticks,
};

pub(super) type AuthoredShowScheduleRowQuery<'world, 'state> = Query<
    'world,
    'state,
    (
        Entity,
        Option<&'static ScheduledShowRow>,
        Option<&'static ScheduledShowBreakRow>,
        Option<&'static ShowScheduleRowOrder>,
    ),
    Or<(With<ScheduledShowRow>, With<ScheduledShowBreakRow>)>,
>;

pub(super) fn create_scheduled_show_row_after_authored_confirmation(
    controller: Entity,
    add_confirmations: &Query<&AddShowConfirmationPending>,
    stages: &Query<&ShowStage>,
    world_definitions: WorldDefinitionsView<'_>,
    scheduled_rows: &AuthoredShowScheduleRowQuery<'_, '_>,
    stage_members: &Query<&WorldMember, With<ShowStage>>,
    commands: &mut Commands,
    persistent_ids: &mut MessageWriter<AssignPersistentId>,
) {
    let Ok(confirmation) = add_confirmations.get(controller) else {
        return;
    };
    let stage = confirmation.stage;
    let Ok(show_stage) = stages.get(stage) else {
        return;
    };
    let Some(show_stage_definition) =
        world_definitions.find_show_stage(show_stage.show_stage_definition)
    else {
        return;
    };
    let trick_performance_capacity = show_stage_definition.schedule_slots;
    let next_order = next_show_schedule_row_order(stage, scheduled_rows);
    let mut spawned = commands.spawn((
        ScheduledShowRow { stage },
        ScheduledShowPerformancePlan::with_trick_performance_capacity(trick_performance_capacity),
        ShowScheduleRowOrder(next_order),
    ));
    let show = spawned.id();
    if let Ok(world_member) = stage_members.get(stage) {
        spawned.insert(*world_member);
        persistent_ids.write(AssignPersistentId {
            entity: show,
            root: world_member.root,
        });
    }
    commands.entity(controller).insert((
        SelectedShowScheduleRow(show),
        EditedShow(show),
        ShowEditing,
    ));
    commands
        .entity(controller)
        .remove::<AddShowConfirmationPending>();
}

pub(super) fn request_authored_confirmation_for_selected_show_schedule_row_deletion(
    controller: Entity,
    selected_schedule_row: Option<Entity>,
    scheduler_selection: &Query<&SelectedShowScheduleRow>,
    commands: &mut Commands,
) {
    let selected_schedule_row = selected_schedule_row.or_else(|| {
        scheduler_selection
            .get(controller)
            .ok()
            .map(|selection| selection.0)
    });
    if let Some(entry) = selected_schedule_row {
        commands
            .entity(controller)
            .insert(DeleteShowConfirmationPending { entry });
    }
}

pub(super) fn delete_confirmed_show_schedule_row_and_close_editor_selection(
    controller: Entity,
    delete_confirmations: &Query<&DeleteShowConfirmationPending>,
    scheduled_rows: &AuthoredShowScheduleRowQuery<'_, '_>,
    commands: &mut Commands,
) {
    let Ok(confirmation) = delete_confirmations.get(controller) else {
        return;
    };
    if let Ok((_, show, pause, order)) = scheduled_rows.get(confirmation.entry) {
        let stage = show
            .map(|show| show.stage)
            .or_else(|| pause.map(|pause| pause.stage));
        let Some(stage) = stage else {
            return;
        };
        if let Some(order) = order {
            for (entry, show, pause, later_order) in scheduled_rows {
                let same_stage = show
                    .map(|show| show.stage)
                    .or_else(|| pause.map(|pause| pause.stage))
                    == Some(stage);
                if same_stage && later_order.is_some_and(|later_order| later_order.0 > order.0) {
                    commands.entity(entry).insert(ShowScheduleRowOrder(
                        later_order.expect("checked above").0 - 1,
                    ));
                }
            }
        }
        commands.entity(confirmation.entry).despawn();
    }
    commands.entity(controller).remove::<(
        SelectedShowScheduleRow,
        DeleteShowConfirmationPending,
        EditedShow,
    )>();
}

pub(super) fn show_selected_scheduled_show_in_read_only_editor(
    controller: Entity,
    selected_schedule_row: Option<Entity>,
    scheduler_selection: &Query<&SelectedShowScheduleRow>,
    scheduled_rows: &AuthoredShowScheduleRowQuery<'_, '_>,
    commands: &mut Commands,
) {
    let selected_schedule_row = selected_schedule_row.or_else(|| {
        scheduler_selection
            .get(controller)
            .ok()
            .map(|selection| selection.0)
    });
    let Some(selection) = selected_schedule_row else {
        return;
    };
    if scheduled_rows
        .get(selection)
        .is_ok_and(|(_, show, _, _)| show.is_some())
    {
        commands.entity(controller).insert(EditedShow(selection));
        commands.entity(controller).remove::<ShowEditing>();
    }
}

pub(super) fn create_show_schedule_break_row_for_selected_stage(
    stage: Entity,
    scheduled_rows: &AuthoredShowScheduleRowQuery<'_, '_>,
    world_definitions: WorldDefinitionsView<'_>,
    stage_members: &Query<&WorldMember, With<ShowStage>>,
    commands: &mut Commands,
    persistent_ids: &mut MessageWriter<AssignPersistentId>,
) {
    let next_order = next_show_schedule_row_order(stage, scheduled_rows);
    let duration_ticks = world_definitions
        .show_scheduling()
        .map_or(1, |show_scheduling| {
            convert_authored_nanoseconds_to_fixed_simulation_ticks(
                show_scheduling.between_show_ns,
                world_definitions,
            )
            .max(1)
        });
    let mut spawned = commands.spawn((
        ScheduledShowBreakRow {
            stage,
            duration_ticks,
        },
        ShowScheduleRowOrder(next_order),
    ));
    if let Ok(world_member) = stage_members.get(stage) {
        let entry = spawned.id();
        spawned.insert(*world_member);
        persistent_ids.write(AssignPersistentId {
            entity: entry,
            root: world_member.root,
        });
    }
}

pub(super) fn move_selected_show_schedule_row_by_one_position(
    controller: Entity,
    direction: i8,
    scheduler_selection: &Query<&SelectedShowScheduleRow>,
    scheduled_rows: &AuthoredShowScheduleRowQuery<'_, '_>,
    commands: &mut Commands,
) {
    let Ok(selected) = scheduler_selection.get(controller) else {
        return;
    };
    let Ok((_, show, pause, Some(order))) = scheduled_rows.get(selected.0) else {
        return;
    };
    let Some(stage) = show
        .map(|show| show.stage)
        .or_else(|| pause.map(|pause| pause.stage))
    else {
        return;
    };
    let target_order = if direction < 0 {
        order.0.checked_sub(1)
    } else {
        order.0.checked_add(1)
    };
    let Some(target_order) = target_order else {
        return;
    };
    let Some((other, _, _, _)) = scheduled_rows.iter().find(|(_, show, pause, other_order)| {
        show.map(|show| show.stage)
            .or_else(|| pause.map(|pause| pause.stage))
            == Some(stage)
            && other_order.is_some_and(|other_order| other_order.0 == target_order)
    }) else {
        return;
    };
    commands
        .entity(selected.0)
        .insert(ShowScheduleRowOrder(target_order));
    commands.entity(other).insert(ShowScheduleRowOrder(order.0));
}

fn next_show_schedule_row_order(
    stage: Entity,
    scheduled_rows: &AuthoredShowScheduleRowQuery<'_, '_>,
) -> u16 {
    scheduled_rows
        .iter()
        .filter_map(|(_, show, pause, order)| {
            let owning_stage = show
                .map(|show| show.stage)
                .or_else(|| pause.map(|pause| pause.stage));
            (owning_stage == Some(stage)).then_some(order.map_or(0, |order| order.0))
        })
        .max()
        .map_or(0, |order| order.saturating_add(1))
}
