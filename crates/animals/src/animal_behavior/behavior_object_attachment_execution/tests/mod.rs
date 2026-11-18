use super::*;
use openzt2_game_data::{
    animation::animation_text_key::AuthoredAnimationTextAction,
    world_definitions::{document::WorldDefinitionDocument, world_objects::*},
};
use openzt2_world_state::animation_graph::animation_graph_playback_message_types::AnimationClipPlaybackRequestRejected;
use openzt2_world_state::animation_graph::animation_presentation_relationship_types::AnimationPresentationOwner;
use openzt2_world_state::animation_playback::animation_event_message_types::AnimationCompleted;
use openzt2_world_state::animation_playback::animation_event_message_types::AnimationObjectCommand;
use openzt2_world_state::world_spawn::persistent_id_types::{PersistentId, PersistentIdAllocator};

fn object(id: AssetId) -> WorldObjectDefinition {
    WorldObjectDefinition {
        audio_cue_prefixes: Vec::new(),
        surface_effect_strength: None,
        presentation_caustic_policies: Vec::new(),
        id,
        kind: WorldObjectKind::Scenery,
        water_filter: None,
        supports_show_tricks: false,
        behavior_type_identifiers: Vec::new(),
        view_data: Vec::new(),
        detach_actions: vec![],
        selected_ui_broadcasts: vec![],
        information_panel: AssetId::default(),
        view_class: None,
        biome_automatic_placement_class: None,
        name_key: id,
        description_key: id,
        zoopedia_subject: id,
        prefab: id,
        catalogue_preview_prefab: None,
        presentation_attachments: vec![],
        named_physical_presentations: vec![],
        interaction_slots: vec![],
        container_quantity: None,
        transactions: vec![],
        prefab_scale: 1.0,
        icon: id,
        biomes: vec![],
        location: id,
        preview_offset_cm: [0; 3],
        preview_scale: 1.0,
        terrain_fitted: false,
        terrain_fit_height_offset_m: 0.0,
        terrain_fit_water_offset_m: None,
        real_physics_water_impact: None,
        price_cents: 0,
        upkeep_cents_per_month: 0,
        properties: Default::default(),
        affordances: Default::default(),
        destruction: None,
        virtual_nodes: Vec::new(),
    }
}

#[test]
fn detach_transfers_inventory_and_kills_only_on_the_later_detach_event() {
    let holder_id = AssetId::from_key("guest");
    let item_id = AssetId::from_key("fruitcup");
    let inventory = AssetId::from_key("inventory");
    let kill = AssetId::from_key("killitem");
    let replace = AssetId::from_key("replaceitem");
    let replacement_id = AssetId::from_key("replacement");
    let mut holder = object(holder_id);
    holder.interaction_slots = [AssetId::default(), inventory]
        .into_iter()
        .map(|tag| WorldObjectInteractionSlotDefinition {
            reservation_tag: tag,
            is_queue: false,
            capacity: 1,
            exclusive_identifier: AssetId::default(),
            owns_contents: tag == inventory,
            hides_contents: tag == inventory,
            entrance_behavior_set: AssetId::default(),
            use_behavior_set: AssetId::default(),
            exit_behavior_set: AssetId::default(),
            target_node_name: String::new(),
        })
        .collect();
    let mut item = object(item_id);
    item.detach_actions = vec![
        WorldObjectDetachActionDefinition {
            name: inventory,
            destination: WorldObjectDetachDestination::Container(inventory),
            created_objects: vec![],
        },
        WorldObjectDetachActionDefinition {
            name: kill,
            destination: WorldObjectDetachDestination::Kill,
            created_objects: vec![],
        },
        WorldObjectDetachActionDefinition {
            name: replace,
            destination: WorldObjectDetachDestination::Kill,
            created_objects: vec![(
                replacement_id,
                WorldObjectDetachDestination::Container(AssetId::default()),
            )],
        },
    ];
    let document = WorldDefinitionDocument {
        objects: vec![holder, item, object(replacement_id)],
        ..Default::default()
    };
    let mut app = App::new();
    events::register_attachment_animation_observers(&mut app);
    app.insert_resource(PersistentIdAllocator::new(Entity::PLACEHOLDER))
        .init_resource::<WorldDefinitions>()
        .init_resource::<Assets<WorldDefinitionAsset>>()
        .add_observer(detach::record_slots_of_destroyed_holders)
        .add_message::<AnimationObjectCommand>()
        .add_message::<AnimationCompleted>()
        .add_message::<AnimationClipPlaybackRequestRejected>()
        .add_systems(
            FixedUpdate,
            (
                events::receive_object_animation_commands,
                detach::release_objects_from_destroyed_holders,
            )
                .chain(),
        );
    let handle = app
        .world_mut()
        .resource_mut::<Assets<WorldDefinitionAsset>>()
        .add(WorldDefinitionAsset::from_test_document(document.clone()));
    app.world_mut()
        .resource_mut::<WorldDefinitions>()
        .index_test_document(handle, &document);
    let actor = app
        .world_mut()
        .spawn((DefinitionId(holder_id), GlobalTransform::IDENTITY))
        .id();
    let controller = app
        .world_mut()
        .spawn((AnimationPresentationOwner {
            gameplay_entity: actor,
        }, openzt2_world_state::animation_playback::animation_playback_controller_types::AnimationPlaybackController {
            playback_generation: 1,
            explicit_clip_request_id: None,
            animation_set_asset: Handle::default(),
            animation_clip_asset_key: "idle".into(),
            elapsed_milliseconds: 0,
                    fractional_milliseconds: default(),
            playback_speed_permille: 1000,
            playback_state: Default::default(),
            playback_repetition_policy: Default::default(),
            playback_clock: Default::default(),
        }))
        .id();
    let carried = app
        .world_mut()
        .spawn((
            DefinitionId(item_id),
            Visibility::Inherited,
            ContainedObject {
                holder_definition: holder_id,
                detach_rule: inventory,
                joint: Some(actor),
                relative_transform: Transform::IDENTITY,
                attachment_request: None,
            },
        ))
        .id();
    app.world_mut()
        .entity_mut(carried)
        .insert(InteractionContainerMember {
            container: actor,
            slot: 0,
        });
    app.world_mut().write_message(AnimationCompleted {
        animation_playback_controller_entity: controller,
        playback_generation: 1,
        explicit_clip_request_id: Some(0),
    });
    app.world_mut().run_schedule(FixedUpdate);
    assert!(app.world().get_entity(carried).is_ok());
    assert_eq!(
        app.world()
            .get::<InteractionContainerMember>(carried)
            .map(|membership| (membership.container, membership.slot)),
        Some((actor, 0))
    );
    let detach = || AnimationObjectCommand {
        controller,
        playback_generation: 1,
        request_id: None,
        joint: actor,
        action: AuthoredAnimationTextAction::DetachObject,
    };
    app.world_mut().write_message(detach());
    app.world_mut().run_schedule(FixedUpdate);
    assert_eq!(
        app.world()
            .get::<InteractionContainerMember>(carried)
            .map(|membership| (membership.container, membership.slot)),
        Some((actor, 1))
    );
    assert_eq!(
        *app.world().get::<Visibility>(carried).unwrap(),
        Visibility::Hidden
    );
    let overflow = app
        .world_mut()
        .spawn((
            DefinitionId(item_id),
            ContainedObject {
                holder_definition: holder_id,
                detach_rule: inventory,
                joint: Some(actor),
                relative_transform: Transform::IDENTITY,
                attachment_request: None,
            },
        ))
        .id();
    app.world_mut()
        .entity_mut(overflow)
        .insert(InteractionContainerMember {
            container: actor,
            slot: 0,
        });
    app.world_mut().write_message(detach());
    app.world_mut().run_schedule(FixedUpdate);
    assert!(
        app.world().get_entity(overflow).is_err(),
        "native detach removes an item rejected by a full destination"
    );
    assert_eq!(
        app.world()
            .get::<InteractionContainerMember>(carried)
            .map(|membership| (membership.container, membership.slot)),
        Some((actor, 1)),
        "existing inventory survives overflow"
    );
    let money = app
        .world_mut()
        .spawn((
            DefinitionId(item_id),
            ContainedObject {
                holder_definition: holder_id,
                detach_rule: kill,
                joint: Some(actor),
                relative_transform: Transform::IDENTITY,
                attachment_request: None,
            },
        ))
        .id();
    app.world_mut()
        .entity_mut(money)
        .insert(InteractionContainerMember {
            container: actor,
            slot: 0,
        });
    app.world_mut().write_message(detach());
    app.world_mut().run_schedule(FixedUpdate);
    assert!(app.world().get_entity(money).is_err());
    assert!(app.world().get_entity(carried).is_ok());
    for (rule, survives) in [
        (AssetId::default(), true),
        (AssetId::from_key("unknown"), false),
    ] {
        let item = app
            .world_mut()
            .spawn((
                DefinitionId(item_id),
                ContainedObject {
                    holder_definition: holder_id,
                    detach_rule: rule,
                    joint: Some(actor),
                    relative_transform: Transform::IDENTITY,
                    attachment_request: None,
                },
            ))
            .id();
        app.world_mut()
            .entity_mut(item)
            .insert(InteractionContainerMember {
                container: actor,
                slot: 0,
            });
        app.world_mut().write_message(detach());
        app.world_mut().run_schedule(FixedUpdate);
        assert_eq!(
            app.world().get_entity(item).is_ok(),
            survives,
            "an empty detach rule releases; an unknown named rule removes"
        );
        assert!(app
            .world()
            .get::<InteractionContainerMember>(item)
            .is_none());
        assert!(app.world().get::<ContainedObject>(item).is_none());
        assert!(app.world().get_entity(carried).is_ok());
    }
    let replaceable = app
        .world_mut()
        .spawn((
            DefinitionId(item_id),
            WorldMember {
                root: Entity::PLACEHOLDER,
            },
            ContainedObject {
                holder_definition: holder_id,
                detach_rule: replace,
                joint: Some(actor),
                relative_transform: Transform::IDENTITY,
                attachment_request: None,
            },
            InteractionContainerMember {
                container: actor,
                slot: 0,
            },
        ))
        .id();
    app.world_mut().write_message(detach());
    app.world_mut().write_message(detach());
    app.world_mut().run_schedule(FixedUpdate);
    assert!(app.world().get_entity(replaceable).is_err());
    let world = app.world_mut();
    let mut created = world.query::<(Entity, &PersistentId, &DefinitionId)>();
    let (replacement, _, definition) = created
        .single(world)
        .expect("the first key should create exactly one replacement");
    assert_eq!(definition.0, replacement_id);
    assert!(world
        .get::<InteractionContainerMember>(replacement)
        .is_none());
    assert!(world.get::<ContainedObject>(replacement).is_none());

    let unowned = app
        .world_mut()
        .spawn((
            DefinitionId(item_id),
            Visibility::Inherited,
            ContainedObject {
                holder_definition: holder_id,
                detach_rule: kill,
                joint: Some(actor),
                relative_transform: Transform::IDENTITY,
                attachment_request: None,
            },
        ))
        .id();
    app.world_mut()
        .entity_mut(unowned)
        .insert(InteractionContainerMember {
            container: actor,
            slot: 0,
        });
    app.world_mut().entity_mut(actor).despawn();
    app.world_mut().run_schedule(FixedUpdate);
    assert!(
        app.world().get_entity(carried).is_err(),
        "owned inventory follows holder lifetime"
    );
    assert!(
        app.world().get_entity(unowned).is_ok(),
        "non-owning hand slot must release its item"
    );
    assert!(app.world().get::<ContainedObject>(unowned).is_none());
    assert!(app
        .world()
        .get::<InteractionContainerMember>(unowned)
        .is_none());
}
