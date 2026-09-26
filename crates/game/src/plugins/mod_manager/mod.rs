mod mod_manager_types;
mod mod_manager_ui_presentation;

use bevy::prelude::*;

use self::mod_manager_types::AssetArchiveReloadState;

pub(super) struct ModManagerPlugin;

impl Plugin for ModManagerPlugin {
    fn build(&self, game_application: &mut App) {
        game_application
            .init_resource::<AssetArchiveReloadState>()
            .configure_sets(
                Update,
                crate::plugins::ui::UiSet::Interaction
                    .run_if(asset_archive_reload_is_not_in_progress),
            )
            .add_systems(
                Startup,
                mod_manager_ui_presentation::spawn_mod_manager_overlay_from_asset_archives,
            )
            .add_systems(
                Update,
                (
                    mod_manager_ui_presentation::synchronize_mod_manager_visibility_and_reload_input_blocking,
                    mod_manager_ui_presentation::toggle_selected_asset_archive_enabled_state,
                    mod_manager_ui_presentation::finish_asset_archive_reload_and_request_main_menu_document_rebuild,
                    mod_manager_ui_presentation::synchronize_mod_manager_archive_checkbox_labels,
                    mod_manager_ui_presentation::present_asset_archive_checkbox_interaction_state,
                )
                    .chain()
                    .in_set(crate::application_schedule::GameSet::Ui)
                    .after(crate::plugins::ui::UiSet::Interaction),
            );
    }
}

pub(crate) fn asset_archive_reload_is_not_in_progress(
    reload_state: Res<AssetArchiveReloadState>,
) -> bool {
    !reload_state.is_in_progress()
}
