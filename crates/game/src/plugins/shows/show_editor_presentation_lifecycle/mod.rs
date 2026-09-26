use bevy::prelude::*;
use openzt2_game_data::ui_document::document::UiDocumentRole;

use crate::plugins::{
    immersive_modes::immersive_mode_state_types::{ActiveImmersiveMode, ImmersiveMode},
    ui::ui_document_lifecycle_contracts::{HideUiDocument, ShowUiRole},
};

pub(super) fn show_authored_show_editor_for_entered_show_edit_mode(
    active: Query<(Entity, &ActiveImmersiveMode), Added<ActiveImmersiveMode>>,
    mut show: MessageWriter<ShowUiRole>,
) {
    for (controller, active) in &active {
        if active.mode == ImmersiveMode::ShowEdit {
            show.write(ShowUiRole {
                role: UiDocumentRole::ShowEditor,
                owner: controller,
            });
        }
    }
}

pub(super) fn hide_authored_show_editor_after_immersive_mode_exit(
    mut removed: RemovedComponents<ActiveImmersiveMode>,
    mut hide: MessageWriter<HideUiDocument>,
) {
    for controller in removed.read() {
        hide.write(HideUiDocument { owner: controller });
    }
}
