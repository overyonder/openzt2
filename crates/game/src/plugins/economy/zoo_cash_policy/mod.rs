use bevy::prelude::*;

use crate::{
    game_session_types::WorldSessionMode,
    plugins::world_spawn::selected_world_identity::SelectedWorldIdentity,
};

use super::zoo_cash_types::UnlimitedZooCash;

pub(super) fn apply_selected_world_unlimited_cash_policy(
    selected_worlds: Query<&SelectedWorldIdentity, Added<SelectedWorldIdentity>>,
    mut commands: Commands,
) {
    for selected_world in &selected_worlds {
        if selected_world.mode == WorldSessionMode::Freeform {
            commands.insert_resource(UnlimitedZooCash);
        } else {
            commands.remove_resource::<UnlimitedZooCash>();
        }
    }
}

pub(super) fn clear_unlimited_cash_policy_after_leaving_game(mut commands: Commands) {
    commands.remove_resource::<UnlimitedZooCash>();
}
