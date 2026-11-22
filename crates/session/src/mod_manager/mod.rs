mod archive_reload_operation;
pub mod mod_manager_types;
mod mod_manager_ui_presentation;
mod redo_button;

use bevy::prelude::*;

use self::archive_reload_operation::AssetArchiveReloadOperation;

pub struct ModManagerPlugin;

impl Plugin for ModManagerPlugin {
    fn build(&self, game_application: &mut App) {
        archive_reload_operation::register_archive_reload_tracking(game_application);
        game_application
            .add_plugins(redo_button::RedoButtonPlugin)
            .configure_sets(
                Update,
                openzt2_interface_state::ui::UiSet::Interaction
                    .run_if(asset_archive_reload_is_not_in_progress),
            )
            .add_systems(
                Startup,
                mod_manager_ui_presentation::spawn_mod_manager_overlay_from_asset_archives,
            )
            .add_systems(
                Update,
                (
                    mod_manager_ui_presentation::apply_mod_manager_visibility_controls,
                    mod_manager_ui_presentation::synchronize_mod_manager_visibility_and_reload_input_blocking,
                    mod_manager_ui_presentation::toggle_selected_asset_archive_enabled_state,
                    mod_manager_ui_presentation::synchronize_mod_manager_archive_checkbox_labels,
                    mod_manager_ui_presentation::present_asset_archive_checkbox_interaction_state,
                )
                    .chain()
                    .in_set(openzt2_world_state::application_schedule::GameSet::Ui)
                    .after(openzt2_interface_state::ui::UiSet::Interaction),
            );
    }
}

fn asset_archive_reload_is_not_in_progress(
    reloads: Query<(), With<AssetArchiveReloadOperation>>,
) -> bool {
    reloads.is_empty()
}
