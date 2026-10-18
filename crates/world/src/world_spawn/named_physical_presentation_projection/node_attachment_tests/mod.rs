use bevy::prelude::*;
use openzt2_game_data::AssetId;

use super::{attach_named_presentations_to_model_nodes, PendingNamedPresentationNodeAttachment};
use openzt2_world_state::world_spawn::{
    prefab_authored_attachment_identifier::PrefabAuthoredAttachmentIdentifier,
    prefab_source_asset_handle::PrefabSourceAssetHandle,
};

fn spawn_owner_with_node(world: &mut World, node: &str) -> (Entity, Entity) {
    let owner = world.spawn(PrefabSourceAssetHandle(Handle::default())).id();
    let node = world
        .spawn((
            ChildOf(owner),
            PrefabAuthoredAttachmentIdentifier(AssetId::from_key(node)),
        ))
        .id();
    (owner, node)
}

fn spawn_holder(world: &mut World, owner: Entity, node: &str) -> Entity {
    world
        .spawn((
            ChildOf(owner),
            PendingNamedPresentationNodeAttachment {
                owner,
                node: node.to_owned(),
            },
        ))
        .id()
}

#[test]
fn holder_moves_onto_the_prefab_node_with_its_authored_name() {
    let mut application = App::new();
    application.add_systems(Update, attach_named_presentations_to_model_nodes);
    let world = application.world_mut();
    let (owner, node) = spawn_owner_with_node(world, "link_fire01");
    let holder = spawn_holder(world, owner, "Link_Fire01");

    application.update();

    let world = application.world();
    assert_eq!(
        world.get::<ChildOf>(holder).map(ChildOf::parent),
        Some(node)
    );
    assert!(world
        .get::<PendingNamedPresentationNodeAttachment>(holder)
        .is_none());
}

#[test]
fn holder_follows_an_animated_joint_with_its_name() {
    let mut application = App::new();
    application.add_systems(Update, attach_named_presentations_to_model_nodes);
    let world = application.world_mut();
    let (owner, _) = spawn_owner_with_node(world, "link_fire01");
    let joint = world
        .spawn((ChildOf(owner), Name::new("Bip01 Spine1")))
        .id();
    let holder = spawn_holder(world, owner, "bip01 spine1");

    application.update();

    assert_eq!(
        application
            .world()
            .get::<ChildOf>(holder)
            .map(ChildOf::parent),
        Some(joint)
    );
}

#[test]
fn holder_waits_for_a_node_from_a_model_that_loads_later() {
    let mut application = App::new();
    application.add_systems(Update, attach_named_presentations_to_model_nodes);
    let world = application.world_mut();
    let (owner, counter) = spawn_owner_with_node(world, "link_counter");
    let holder = spawn_holder(world, owner, "Link_Smokestack");

    application.update();
    assert_eq!(
        application
            .world()
            .get::<ChildOf>(holder)
            .map(ChildOf::parent),
        Some(owner),
        "the smokestack node does not exist yet"
    );

    let smokestack_node = application
        .world_mut()
        .spawn((
            ChildOf(counter),
            PrefabAuthoredAttachmentIdentifier(AssetId::from_key("link_smokestack")),
        ))
        .id();
    application.update();

    let world = application.world();
    assert_eq!(
        world.get::<ChildOf>(holder).map(ChildOf::parent),
        Some(smokestack_node)
    );
    assert!(world
        .get::<PendingNamedPresentationNodeAttachment>(holder)
        .is_none());
}
