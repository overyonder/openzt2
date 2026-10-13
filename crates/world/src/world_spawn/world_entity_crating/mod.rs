use bevy::prelude::*;
use openzt2_world_state::world_spawn::world_entity_crating::RemoveWorldEntityFromCrate;
use openzt2_world_state::world_spawn::world_entity_crating::WorldEntityIsCrated;

pub(super) fn remove_requested_world_entities_from_crates(
    mut commands: Commands,
    mut requests: MessageReader<RemoveWorldEntityFromCrate>,
    crated_world_entities: Query<(), With<WorldEntityIsCrated>>,
) {
    for request in requests.read() {
        if crated_world_entities.contains(request.0) {
            commands.entity(request.0).remove::<WorldEntityIsCrated>();
        }
    }
}
