use super::persistent_id_assignment::{
    assign_requested_persistent_ids, AssignPersistentId, PersistentIdAssigned,
    PersistentIdAssignmentFailed,
};
use super::persistent_id_types::{PersistentId, PersistentIdAllocator};
use super::world_membership_types::WorldMember;
use bevy::prelude::*;

#[test]
fn requested_identity_is_inserted_once_before_completion_is_observed() {
    let mut app = App::new();
    app.add_message::<AssignPersistentId>()
        .add_message::<PersistentIdAssigned>()
        .add_message::<PersistentIdAssignmentFailed>()
        .add_systems(Update, assign_requested_persistent_ids);
    let root = app.world_mut().spawn_empty().id();
    let entity = app.world_mut().spawn(WorldMember { root }).id();
    app.insert_resource(PersistentIdAllocator::new(root));
    app.world_mut()
        .resource_mut::<Messages<AssignPersistentId>>()
        .write(AssignPersistentId { entity, root });

    app.world_mut()
        .resource_mut::<Messages<AssignPersistentId>>()
        .write(AssignPersistentId { entity, root });

    app.update();

    assert_eq!(
        app.world().get::<PersistentId>(entity),
        Some(&PersistentId(1))
    );
    assert_eq!(app.world().resource::<PersistentIdAllocator>().next(), 2);
    assert_eq!(
        app.world_mut()
            .resource_mut::<Messages<PersistentIdAssignmentFailed>>()
            .drain()
            .filter(|failure| failure.entity == entity)
            .count(),
        1
    );
    let assigned = app
        .world_mut()
        .resource_mut::<Messages<PersistentIdAssigned>>()
        .drain()
        .collect::<Vec<_>>();
    assert_eq!(
        assigned,
        vec![PersistentIdAssigned {
            entity,
            id: PersistentId(1)
        }]
    );

    app.world_mut()
        .resource_mut::<Messages<AssignPersistentId>>()
        .write(AssignPersistentId { entity, root });
    app.update();
    assert_eq!(app.world().resource::<PersistentIdAllocator>().next(), 2);
    assert!(app
        .world_mut()
        .resource_mut::<Messages<PersistentIdAssignmentFailed>>()
        .drain()
        .any(|failure| failure.entity == entity));
}
