use bevy::{
    ecs::{entity_disabling::Disabled, query::Allow},
    prelude::*,
};
use openzt2_zoo_state::environment::environment_state_types::WorldEnvironment;

pub(super) fn retire_remaining_world_environments_after_leaving_game(
    mut commands: Commands,
    environments: Query<Entity, (With<WorldEnvironment>, Allow<Disabled>)>,
) {
    for environment in &environments {
        commands.entity(environment).try_despawn();
    }
}
