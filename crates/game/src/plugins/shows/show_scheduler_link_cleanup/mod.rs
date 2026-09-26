use bevy::prelude::*;

use super::{
    show_editor_interaction_types::{DeleteShowConfirmationPending, EditedShow, ShowEditing},
    show_schedule_types::{ScheduledShowBreakRow, ScheduledShowRow, SelectedShowScheduleRow},
    show_stage_types::ShowStage,
};

pub(super) fn remove_show_schedule_entries_and_editor_links_with_missing_owners(
    mut commands: Commands,
    stages: Query<(), With<ShowStage>>,
    entries: Query<
        (
            Entity,
            Option<&ScheduledShowRow>,
            Option<&ScheduledShowBreakRow>,
        ),
        Or<(With<ScheduledShowRow>, With<ScheduledShowBreakRow>)>,
    >,
    controllers: Query<
        (
            Entity,
            Option<&SelectedShowScheduleRow>,
            Option<&EditedShow>,
            Option<&DeleteShowConfirmationPending>,
        ),
        Or<(
            With<SelectedShowScheduleRow>,
            With<EditedShow>,
            With<DeleteShowConfirmationPending>,
        )>,
    >,
) {
    for (entry, show, pause) in &entries {
        let stage = show
            .map(|show| show.stage)
            .or_else(|| pause.map(|pause| pause.stage));
        if stage.is_some_and(|stage| !stages.contains(stage)) {
            commands.entity(entry).despawn();
        }
    }
    for (controller, selected, edited, pending_delete) in &controllers {
        let broken = selected.is_some_and(|selected| !entries.contains(selected.0))
            || edited
                .is_some_and(|edited| !entries.contains(edited.0) && !stages.contains(edited.0))
            || pending_delete.is_some_and(|pending| !entries.contains(pending.entry));
        if broken {
            commands.entity(controller).remove::<(
                SelectedShowScheduleRow,
                EditedShow,
                ShowEditing,
                DeleteShowConfirmationPending,
            )>();
        }
    }
}
