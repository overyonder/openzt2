use super::*;
use crate::animal_behavior::behavior_container_admission::BehaviorAdmissionInstruction;
use bevy::ecs::{entity::Entities, world::EntityWorldMut};
use openzt2_world::world_spawn::prefab_world_instance_spawning::hydrate_loaded_scene_prefab_into_world_instance_root;
use openzt2_world_state::world_spawn::persistent_id_types::PersistentId;
use openzt2_zoo_state::animal_behavior::interaction_container_membership::requests::{
    InteractionContainerAdmissionOutcome, InteractionContainerAdmissionRequest,
    ReleaseInteractionContainerMember,
};
use openzt2_zoo_state::behavior_task_execution_types::BehaviorLocomotionModifier;

pub(super) struct NewAttachedObject {
    pub(super) world: WorldMember,
    pub(super) definition: AssetId,
    pub(super) instance_transform: Transform,
    pub(super) prefab: Option<Handle<ScenePrefabAsset>>,
}

pub(super) enum AttachmentAfterAdmission {
    Authored { tick: u64, detach: bool },
    FoodProp(held_food_prop::FoodPropAdmission),
}

pub(super) struct AttachAfterAdmission {
    pub(super) actor: Entity,
    pub(super) instruction: BehaviorAdmissionInstruction,
    pub(super) attachment: ContainedObject,
    pub(super) visibility: Visibility,
    pub(super) modifier: Option<AssetId>,
    pub(super) spawn: Option<NewAttachedObject>,
    pub(super) finish: AttachmentAfterAdmission,
}

pub(super) enum AttachmentAdmission {
    Attach(AttachAfterAdmission),
    Detach(DetachAdmission),
}

pub(super) enum DetachAdmission {
    Transfer(ContainedObject, Visibility),
    Create {
        holder: Entity,
        definition: AssetId,
        world: WorldMember,
        transform: GlobalTransform,
        attachment: ContainedObject,
    },
}

#[derive(EntityEvent)]
pub(super) struct InteractionContainerAttachmentAdmission {
    #[event_target]
    request_entity: Entity,
    outcome: InteractionContainerAdmissionOutcome,
}

#[derive(Component)]
pub(super) struct PendingAttachmentAdmission(Option<AttachmentAdmission>);

pub(super) fn request_existing_object_admission(
    commands: &mut Commands,
    item: Entity,
    holder: Entity,
    slot: usize,
    capacity: u8,
    admission: AttachmentAdmission,
) {
    request_attachment_admission(
        commands,
        InteractionContainerAdmissionRequest::Admit {
            container: holder,
            member: item,
            slots: vec![(slot, capacity)],
            despawn_on_refusal: false,
            queue_service_slots: Vec::new(),
        },
        admission,
    );
}

pub(super) fn request_new_object_admission(
    commands: &mut Commands,
    holder: Entity,
    slot: usize,
    capacity: u8,
    world_root: Entity,
    admission: AttachmentAdmission,
) {
    request_attachment_admission(
        commands,
        InteractionContainerAdmissionRequest::AdmitNew {
            container: holder,
            slot,
            capacity,
            world_root,
        },
        admission,
    );
}

fn request_attachment_admission(
    commands: &mut Commands,
    request: InteractionContainerAdmissionRequest,
    admission: AttachmentAdmission,
) {
    let operation = commands
        .spawn(PendingAttachmentAdmission(Some(admission)))
        .id();
    commands.queue(move |world: &mut World| {
        let Some(pending) = world.get::<PendingAttachmentAdmission>(operation) else {
            return;
        };
        if matches!(&pending.0, Some(AttachmentAdmission::Attach(attach)) if !attach.instruction.is_current(world)) {
            world.entity_mut(operation).despawn();
            return;
        }
        let outcome = request.commit(world);
        world.trigger(InteractionContainerAttachmentAdmission { request_entity: operation, outcome });
    });
}

pub(super) enum DetachMembershipRequest {
    Admit(InteractionContainerAdmissionRequest, DetachAdmission),
    Release { member: Entity, despawn: bool },
}

impl DetachMembershipRequest {
    pub(super) fn submit(self, commands: &mut Commands) {
        match self {
            Self::Admit(request, admission) => request_attachment_admission(
                commands,
                request,
                AttachmentAdmission::Detach(admission),
            ),
            Self::Release { member, despawn } => {
                commands.queue(ReleaseInteractionContainerMember { member, despawn });
            }
        }
    }
}

pub(super) fn apply_object_attachment_admissions(
    mut commands: Commands,
    result: On<InteractionContainerAttachmentAdmission>,
    mut pending: Query<&mut PendingAttachmentAdmission>,
    mut tasks: Query<&mut BehaviorTaskExecutionState>,
    prefabs: Res<Assets<ScenePrefabAsset>>,
    mut playback: MessageWriter<AnimationClipPlaybackRequest>,
    entities: &Entities,
) {
    use InteractionContainerAdmissionOutcome::{Granted, Refused};
    let Ok(mut pending) = pending.get_mut(result.request_entity) else {
        return;
    };
    let Some(admission) = pending.0.take() else {
        return;
    };
    commands.entity(result.request_entity).despawn();
    match (admission, result.outcome) {
        (AttachmentAdmission::Detach(admission), outcome) => {
            apply_detach_admission(admission, outcome, &mut commands);
        }
        (
            AttachmentAdmission::Attach(prepared),
            Granted {
                member,
                slot,
                persistent_id,
                ..
            },
        ) => {
            let AttachAfterAdmission {
                actor,
                instruction: _,
                attachment,
                visibility,
                modifier,
                spawn,
                finish,
            } = prepared;
            if let Some(spawn) = spawn {
                let Some(id) = persistent_id else {
                    commands.entity(member).try_despawn();
                    return;
                };
                if !hydrate_admitted_attachment(member, spawn, id, &prefabs, &mut commands) {
                    commands.entity(member).try_despawn();
                    return;
                }
            }
            if !entities.contains_spawned(actor) {
                commands
                    .entity(member)
                    .try_insert(detach::DestroyedHolderSlot(slot));
            }
            if let Some(modifier) = modifier {
                commands
                    .entity(actor)
                    .try_insert(BehaviorLocomotionModifier {
                        modifier,
                        item: member,
                    });
            }
            commands.entity(member).try_insert((attachment, visibility));
            match finish {
                AttachmentAfterAdmission::Authored { tick, detach } => {
                    if detach {
                        commands
                            .entity(member)
                            .try_insert(DeliveredAttachmentDetach);
                    }
                    commands
                        .entity(actor)
                        .try_remove::<PendingObjectAttachment>();
                    if let Ok(mut task) = tasks.get_mut(actor) {
                        advance_behavior_task_to_next_action(&mut task, tick);
                    }
                }
                AttachmentAfterAdmission::FoodProp(food) => {
                    held_food_prop::finish_food_prop_admission(
                        actor,
                        member,
                        food,
                        &mut tasks,
                        &mut playback,
                        &mut commands,
                    );
                }
            }
        }
        (AttachmentAdmission::Attach(prepared), outcome) => {
            if (matches!(outcome, Refused)
                || matches!(prepared.finish, AttachmentAfterAdmission::Authored { .. }))
                && entities.contains_spawned(prepared.actor)
            {
                mark_behavior_task_for_failure_and_stop_navigation(prepared.actor, &mut commands);
            }
        }
    }
}

fn apply_detach_admission(
    admission: DetachAdmission,
    outcome: InteractionContainerAdmissionOutcome,
    commands: &mut Commands,
) {
    use InteractionContainerAdmissionOutcome::{Granted, IdentityUnavailable};

    match (admission, outcome) {
        (DetachAdmission::Transfer(attachment, visibility), Granted { member, .. }) => {
            commands.entity(member).try_insert((attachment, visibility));
        }
        (DetachAdmission::Create { holder, .. }, IdentityUnavailable) => {
            warn!(
                ?holder,
                "cannot allocate persistent identity for detach-created object"
            );
        }
        (
            DetachAdmission::Create {
                holder,
                definition,
                world,
                transform,
                attachment,
            },
            Granted {
                member,
                slot,
                persistent_id: Some(id),
                ..
            },
        ) => {
            commands.entity(member).try_insert((
                created::PendingDetachCreatedObject::default(),
                DefinitionId(definition),
                world,
                id,
                transform.compute_transform(),
                transform,
                attachment,
                Visibility::Hidden,
            ));
            // A membership command can despawn the holder or one of its ancestors.
            commands
                .entity(member)
                .queue_silenced(move |mut item: EntityWorldMut<'_>| {
                    if !item.world().entities().contains_spawned(holder) {
                        item.insert(detach::DestroyedHolderSlot(slot));
                    }
                });
        }
        _ => {}
    }
}

fn hydrate_admitted_attachment(
    member: Entity,
    spawn: NewAttachedObject,
    id: PersistentId,
    prefabs: &Assets<ScenePrefabAsset>,
    commands: &mut Commands,
) -> bool {
    if let Some(handle) = spawn.prefab {
        let Some(prefab) = prefabs.get(&handle) else {
            return false;
        };
        hydrate_loaded_scene_prefab_into_world_instance_root(
            commands,
            member,
            prefab,
            handle,
            spawn.world.root,
            spawn.definition,
            id,
            spawn.instance_transform,
            true,
            None,
            RigidBody::Kinematic,
        );
    } else {
        commands.entity(member).try_insert((
            spawn.world,
            DefinitionId(spawn.definition),
            id,
            Inspectable {
                definition: spawn.definition,
            },
            spawn.instance_transform,
        ));
    }
    true
}
