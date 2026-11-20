use bevy::{
    ecs::{lifecycle::Remove, relationship::RelationshipTarget},
    prelude::*,
};

use openzt2_interface_state::immersive_modes::{
    immersive_mode_message_types::{ExitImmersiveMode, ModeExitReason},
    immersive_mode_state_types::{ActiveImmersiveMode, ImmersiveMode},
};
use openzt2_zoo_state::{
    animal_lifecycle::types::Animal,
    shows::{
        show_schedule_types::{ScheduledPerformerTricks, SelectedShowScheduleRow, ShowStageRows},
        show_stage_types::ShowStage,
    },
};

use super::show_editor_interaction_types::{
    AddShowConfirmationPending, DeleteShowConfirmationPending, EditedShow, PendingShowAdditions,
    ShowEditing, ShowEditors, ShowMixerDropdown,
};

pub(super) fn retire_schedule_and_editor_links_when_stage_is_removed(
    removed: On<Remove<ShowStage>>,
    rows: Query<&ShowStageRows>,
    editors: Query<&ShowEditors>,
    additions: Query<&PendingShowAdditions>,
    mut commands: Commands,
) {
    if let Ok(rows) = rows.get(removed.entity) {
        for row in rows.iter() {
            commands.entity(row).try_despawn();
        }
    }
    if let Ok(editors) = editors.get(removed.entity) {
        for controller in editors.iter() {
            commands.entity(controller).try_remove::<EditedShow>();
        }
    }
    if let Ok(additions) = additions.get(removed.entity) {
        for controller in additions.iter() {
            commands
                .entity(controller)
                .try_remove::<AddShowConfirmationPending>();
        }
    }
}

pub(super) fn remove_trick_slots_when_animal_is_removed(
    removed: On<Remove<Animal>>,
    performers: Query<&ScheduledPerformerTricks>,
    mut commands: Commands,
) {
    if let Ok(tricks) = performers.get(removed.entity) {
        for slot in tricks.iter() {
            commands.entity(slot).try_despawn();
        }
    }
}

pub(super) fn clear_editor_after_schedule_selection_is_removed(
    removed: On<Remove<SelectedShowScheduleRow>>,
    mut commands: Commands,
) {
    commands
        .entity(removed.entity)
        .try_remove::<(EditedShow, DeleteShowConfirmationPending)>();
}

pub(super) fn clear_editing_after_edited_show_is_removed(
    removed: On<Remove<EditedShow>>,
    active_modes: Query<&ActiveImmersiveMode>,
    mut exit: MessageWriter<ExitImmersiveMode>,
    mut commands: Commands,
) {
    if active_modes
        .get(removed.entity)
        .is_ok_and(|active| active.mode == ImmersiveMode::ShowEdit)
    {
        exit.write(ExitImmersiveMode {
            controller: removed.entity,
            reason: ModeExitReason::SubjectRemoved,
        });
    }
    commands
        .entity(removed.entity)
        .try_remove::<(ShowEditing, ShowMixerDropdown)>();
}
