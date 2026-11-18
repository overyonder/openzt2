use super::*;
use bevy::ecs::system::RunSystemOnce;

use super::requests::{
    InteractionContainerAdmission, InteractionContainerAdmissionOutcome,
    InteractionContainerAdmissionRecipient, InteractionContainerAdmissionRequest,
    InteractionContainerRequest, register_interaction_container_requests,
    resolve_interaction_container_requests,
};

fn request_admission(
    requests: &mut MessageWriter<InteractionContainerRequest>,
    container: Entity,
    member: Entity,
    slots: Vec<(usize, u8)>,
) {
    requests.write(InteractionContainerRequest::Admit {
        admission: InteractionContainerAdmissionRequest::Admit {
            container,
            member,
            slots,
            despawn_on_refusal: false,
            queue_service_slots: Vec::new(),
        },
        reply_to: InteractionContainerAdmissionRecipient::Docking(member),
    });
}

fn resolve_admissions(world: &mut World) -> Vec<Option<usize>> {
    world.run_schedule(FixedUpdate);
    world
        .run_system_once(
            |mut results: MessageReader<InteractionContainerAdmission>| {
                results
                    .read()
                    .map(|result| match result.outcome {
                        InteractionContainerAdmissionOutcome::Granted { slot, .. } => Some(slot),
                        InteractionContainerAdmissionOutcome::Refused => None,
                        InteractionContainerAdmissionOutcome::IdentityUnavailable => {
                            panic!("read an identity error for an existing member")
                        }
                    })
                    .collect::<Vec<_>>()
            },
        )
        .unwrap()
}

fn slot(
    index: usize,
    tag: AssetId,
    is_queue: bool,
    capacity: u8,
) -> WorldObjectInteractionSlotDefinition {
    WorldObjectInteractionSlotDefinition {
        reservation_tag: tag,
        is_queue,
        capacity,
        exclusive_identifier: AssetId::default(),
        owns_contents: false,
        hides_contents: false,
        entrance_behavior_set: AssetId::default(),
        use_behavior_set: AssetId::default(),
        exit_behavior_set: AssetId::default(),
        target_node_name: index.to_string(),
    }
}

#[test]
fn service_selection_preserves_membership_and_excludes_queue_full_and_other_tags() {
    let mut app = App::new();
    register_interaction_container_requests(&mut app);
    app.add_systems(FixedUpdate, resolve_interaction_container_requests);
    let world = app.world_mut();
    let target = world.spawn_empty().id();
    let actor = world.spawn_empty().id();
    let other = world.spawn_empty().id();
    let tag = AssetId::from_key("use_stand");
    let slots = [
        slot(0, tag, true, 1),
        slot(1, tag, false, 1),
        slot(2, tag, false, 1),
        slot(3, AssetId::from_key("other"), false, 1),
    ];
    let position = |name: &str| match name {
        "0" => Some(Vec2::ZERO),
        "1" => Some(Vec2::X),
        "2" => Some(Vec2::X * 2.0),
        "3" => Some(Vec2::X * 3.0),
        _ => None,
    };
    let initial_slots = slots.clone();
    world
        .run_system_once(
            move |mut requests: MessageWriter<InteractionContainerRequest>,
                  memberships: InteractionContainerMemberships| {
                let candidates = memberships.service_slot_candidates(
                    target,
                    actor,
                    &initial_slots,
                    Some(tag),
                    position,
                );
                assert_eq!(candidates, [(1, 1), (2, 1)]);
                assert!(memberships
                    .service_slot_candidates(target, actor, &initial_slots, Some(tag), |_| None)
                    .is_empty());
                request_admission(&mut requests, target, other, vec![(1, 1)]);
                request_admission(&mut requests, target, actor, candidates);
                requests.write(InteractionContainerRequest::Release {
                    member: other,
                    despawn: false,
                });
            },
        )
        .unwrap();
    assert_eq!(resolve_admissions(world), [Some(1), Some(2)]);
    world
        .run_system_once(move |memberships: InteractionContainerMemberships| {
            assert_eq!(
                memberships.service_slot_candidates(target, actor, &slots, Some(tag), position),
                [(2, 1), (1, 1)]
            );
        })
        .unwrap();
    assert_eq!(
        world.get::<InteractionContainerMember>(actor),
        Some(&InteractionContainerMember {
            container: target,
            slot: 2
        })
    );
    assert!(world.get::<InteractionContainerMember>(other).is_none());
}

#[test]
fn only_queue_front_can_use_free_capacity_in_its_own_container() {
    let mut app = App::new();
    register_interaction_container_requests(&mut app);
    app.add_systems(FixedUpdate, resolve_interaction_container_requests);
    let world = app.world_mut();
    let target = world.spawn_empty().id();
    let first = world.spawn_empty().id();
    let second = world.spawn_empty().id();
    let third = world.spawn_empty().id();
    let serving = world.spawn_empty().id();
    let tag = AssetId::from_key("food");
    let slots = [
        slot(0, tag, true, 5),
        slot(1, tag, false, 1),
        slot(2, AssetId::from_key("drinks"), false, 1),
    ];
    world
        .run_system_once(
            move |mut requests: MessageWriter<InteractionContainerRequest>| {
                for member in [first, second, third] {
                    request_admission(&mut requests, target, member, vec![(0, 5)]);
                }
                request_admission(&mut requests, target, serving, vec![(1, 1)]);
                request_admission(&mut requests, target, first, vec![(1, 1)]);
                requests.write(InteractionContainerRequest::Release {
                    member: serving,
                    despawn: false,
                });
                request_admission(&mut requests, target, first, vec![(1, 1)]);
                requests.write(InteractionContainerRequest::Release {
                    member: first,
                    despawn: false,
                });
            },
        )
        .unwrap();
    assert_eq!(
        resolve_admissions(world),
        [Some(0), Some(0), Some(0), Some(1), None, Some(1)],
    );
    world
        .run_system_once(move |memberships: InteractionContainerMemberships| {
            assert!(memberships.queue_front_can_enter_service(target, second, 0, &slots));
            assert!(!memberships.queue_front_can_enter_service(target, third, 0, &slots));
        })
        .unwrap();
    assert!(world
        .get::<InteractionContainerMembers>(target)
        .unwrap()
        .iter()
        .eq([second, third]));
}

#[test]
fn a_waiter_sees_capacity_at_its_admission() {
    let mut app = App::new();
    register_interaction_container_requests(&mut app);
    app.add_systems(
        FixedUpdate,
        (
            resolve_interaction_container_requests,
            resolve_interaction_container_requests,
        )
            .chain(),
    );
    let world = app.world_mut();
    let container = world.spawn_empty().id();
    let waiter = world.spawn_empty().id();
    let serving = world
        .spawn(InteractionContainerMember { container, slot: 1 })
        .id();
    world.write_message(InteractionContainerRequest::Admit {
        admission: InteractionContainerAdmissionRequest::Admit {
            container,
            member: waiter,
            slots: vec![(0, 2)],
            despawn_on_refusal: false,
            queue_service_slots: vec![(1, 1)],
        },
        reply_to: InteractionContainerAdmissionRecipient::QueueWait(waiter),
    });
    world.write_message(InteractionContainerRequest::Release {
        member: serving,
        despawn: false,
    });
    world.write_message(InteractionContainerRequest::Admit {
        admission: InteractionContainerAdmissionRequest::Admit {
            container,
            member: waiter,
            slots: vec![(0, 2)],
            despawn_on_refusal: false,
            queue_service_slots: vec![(1, 1)],
        },
        reply_to: InteractionContainerAdmissionRecipient::QueueWait(waiter),
    });

    world.run_schedule(FixedUpdate);
    world
        .run_system_once(
            |mut results: MessageReader<InteractionContainerAdmission>| {
                let available: Vec<_> = results
                    .read()
                    .map(|result| {
                        let InteractionContainerAdmissionOutcome::Granted {
                            queue_service_available,
                            ..
                        } = result.outcome
                        else {
                            panic!("the queue request was refused");
                        };
                        queue_service_available
                    })
                    .collect::<Vec<_>>();
                assert_eq!(available, [false, true]);
            },
        )
        .unwrap();
}
