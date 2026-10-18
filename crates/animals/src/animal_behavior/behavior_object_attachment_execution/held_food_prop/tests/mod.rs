use super::*;

#[test]
fn authored_food_prop_action_keeps_its_detach_continuation() {
    let document = BehaviorDocumentAsset::from_test_source(br#"
        <BehaviorSets><Eat><subjects><Hippopotamus/></subjects><behaviors>
        <BFBehAttachObject attachEntity="s_prop" targetAnim="Chew_Eat"
          targetNode="Dock_Attach" detachAction="killitem" detachBehSet="Chew" container="foodprop"/>
        </behaviors></Eat></BehaviorSets>"#);
    let set = document.behavior_set_at_index(0).unwrap();
    let [BehaviorAction::AttachObject(action)] = set.actions.lowered_actions() else {
        panic!("the authored eating instruction must lower as a complete action");
    };
    assert_eq!(action.detach_behavior_set, Some(AssetId::from_key("chew")));
    assert_eq!(action.target_node.as_deref(), Some("Dock_Attach"));
}

#[test]
fn food_prop_attach_and_detach_keys_share_one_request_and_cancel_with_the_task() {
    let mut app = App::new();
    app.init_resource::<InteractionContainerOccupancy>()
        .add_message::<AnimationObjectCommand>()
        .add_message::<AnimationCompleted>()
        .add_message::<AnimationClipPlaybackRequestRejected>()
        .add_systems(
            Update,
            (
                receive_food_prop_animation_events,
                discard_cancelled_food_prop_attachments,
            )
                .chain(),
        );
    let root = app.world_mut().spawn_empty().id();
    let source = app.world_mut().spawn(WorldMember { root }).id();
    let actor = app.world_mut().spawn_empty().id();
    let mouth = app.world_mut().spawn_empty().id();
    let item = app
        .world_mut()
        .spawn((
            ContainedObject {
                holder_definition: AssetId::from_key("hippopotamus"),
                detach_rule: AssetId::from_key("killitem"),
                joint: Some(source),
                relative_transform: Transform::IDENTITY,
                attachment_request: None,
            },
            FoodPropInstructionOwner(actor),
        ))
        .id();
    let task = BehaviorTaskExecutionState {
        execution_id: 4,
        program: AssetId::from_key("eat"),
        target: Some(source),
        document: Handle::default(),
        declaration: 0,
        phase: BehaviorTaskExecutionPhase::Set,
        action: 0,
        repetitions: 0,
        next_action_tick: 0,
        object: None,
        interaction_slot: None,
        stack: Default::default(),
    };
    let mut origin = PendingObjectAttachment::waiting_for_attachment(&task, None);
    origin.request_id = Some(7);
    app.world_mut().entity_mut(actor).insert((
        task,
        HeldFoodPropAttachment {
            origin,
            item: Some(item),
            attached: false,
            detached: false,
            completed: false,
        },
    ));
    assert!(app
        .world_mut()
        .resource_mut::<InteractionContainerOccupancy>()
        .admit(actor, item, 3, 1));
    let controller = app
        .world_mut()
        .spawn((
            AnimationPresentationOwner {
                gameplay_entity: actor,
            },
            AnimationPlaybackController {
                playback_generation: 2,
                explicit_clip_request_id: Some(7),
                animation_set_asset: Handle::default(),
                animation_clip_asset_key: "Chew_Eat".into(),
                elapsed_milliseconds: 0,
                fractional_milliseconds: default(),
                playback_speed_permille: 1000,
                playback_state: Default::default(),
                playback_repetition_policy: Default::default(),
                playback_clock: Default::default(),
            },
        ))
        .id();
    app.world_mut().write_message(AnimationObjectCommand {
        controller,
        playback_generation: 1,
        request_id: Some(7),
        joint: mouth,
        action: AuthoredAnimationTextAction::AttachPendingObject,
    });
    app.update();
    assert_eq!(
        app.world().get::<ContainedObject>(item).unwrap().joint,
        Some(source)
    );
    for action in [
        AuthoredAnimationTextAction::AttachPendingObject,
        AuthoredAnimationTextAction::DetachObject,
    ] {
        app.world_mut().write_message(AnimationObjectCommand {
            controller,
            playback_generation: 2,
            request_id: Some(7),
            joint: mouth,
            action,
        });
    }
    app.update();
    assert_eq!(
        app.world().get::<ContainedObject>(item).unwrap().joint,
        Some(mouth)
    );
    let pending = app.world().get::<HeldFoodPropAttachment>(actor).unwrap();
    assert!(pending.attached && pending.detached);
    // Interrupting before the continuation executes must discard the temporary
    // prop and its named-container reservation rather than leave a ghost item.
    app.world_mut()
        .entity_mut(actor)
        .remove::<BehaviorTaskExecutionState>();
    app.update();
    assert!(app.world().get_entity(item).is_err());
    assert!(app.world().get::<HeldFoodPropAttachment>(actor).is_none());
    assert!(app
        .world()
        .resource::<InteractionContainerOccupancy>()
        .container_for_member(item)
        .is_none());
}
