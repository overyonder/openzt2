use bevy::prelude::*;
use openzt2_game_data::AssetId;

use super::{
    world_membership_types::{WorldMember, WorldRoot},
    world_unloading::{unload_requested_worlds_and_their_members, UnloadWorld},
};

#[test]
fn unload_removes_only_members_of_the_requested_world() {
    let mut app = App::new();
    app.add_message::<UnloadWorld>()
        .add_systems(Update, unload_requested_worlds_and_their_members);
    let root = app
        .world_mut()
        .spawn(WorldRoot {
            scenario: AssetId::from_key("world"),
        })
        .id();
    let member = app.world_mut().spawn(WorldMember { root }).id();
    let unrelated = app.world_mut().spawn_empty().id();
    app.world_mut()
        .resource_mut::<Messages<UnloadWorld>>()
        .write(UnloadWorld(root));

    app.update();

    assert!(app.world().get_entity(root).is_err());
    assert!(app.world().get_entity(member).is_err());
    assert!(app.world().get_entity(unrelated).is_ok());
}
