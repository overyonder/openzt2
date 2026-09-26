use bevy::prelude::*;

#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct WorldEntityIsCrated;

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct RemoveWorldEntityFromCrate(pub(crate) Entity);

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
