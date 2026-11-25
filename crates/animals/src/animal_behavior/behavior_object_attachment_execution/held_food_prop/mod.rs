//! Food props start at the source node, attach at the clip's text key and
//! detach before the authored continuation returns to the enclosing task.

use bevy::ecs::relationship::RelationshipTarget;
use openzt2_zoo_state::behavior_task_execution_types::running_actions::behavior_action_actors;

use super::super::behavior_subject_type_resolution::{
    behavior_subject_type_identifiers, AnimalBehaviorSubjectTypes,
};
use super::*;
use openzt2_game_assets::behavior::behavior_asset_types::LoadedBehaviorDocumentCollection;
use openzt2_game_data::animation::animation_text_key::AuthoredAnimationTextAction;
use openzt2_world_state::animation_graph::animation_graph_playback_message_types::AnimationClipPlaybackRequestRejected;
use openzt2_world_state::animation_graph::animation_presentation_relationship_types::AnimationPresentationOwner;
use openzt2_world_state::animation_playback::animation_event_message_types::{
    AnimationCompleted, AnimationObjectCommand,
};
use openzt2_world_state::animation_playback::animation_playback_controller_types::AnimationPlaybackController;
use openzt2_world_state::world_spawn::world_entity_crating::{
    AnimalCrate, ScriptedCrating, UncratedWorldEntity,
};
use openzt2_zoo_state::animal_health::types::{Dead, Frozen, Tranquilized};
use openzt2_zoo_state::animal_lifecycle::types::SpeciesHandle;
use openzt2_zoo_state::behavior_task_execution_types::running_actions::{
    RunBehaviorAttachObject, RunningBehaviorInstruction, SelectedBehaviorSet,
};
use openzt2_zoo_state::guests::guest_simulation_types::GuestArchetype;
use openzt2_zoo_state::staff::staff_employment_types::StaffRole;

#[cfg(test)]
mod tests;

#[derive(Component)]
pub(super) struct HeldFoodPropAttachment {
    pub(super) origin: PendingObjectAttachment,
    attached: bool,
    detached: bool,
    completed: bool,
}

pub(super) fn register(app: &mut App) {
    use openzt2_zoo_state::behavior_task_execution_types::running_actions::{
        activated_behavior_actor, update_waiting_behavior_actors,
    };
    app.add_observer(activated_behavior_actor::<openzt2_zoo_state::behavior_task_execution_types::running_actions::RunBehaviorAttachObject>.pipe(execute_food_prop_attachments));

    use openzt2_game_assets::application_lifecycle::GamePhase;
    use openzt2_world_state::application_schedule::FixedGameSet;
    use openzt2_zoo_state::behavior_task_execution_types::running_actions::BehaviorStepFixedSet;
    app.add_observer(cancel_removed_food_prop_instruction)
        .add_observer(cancel_disabled_food_prop_instruction)
        .add_observer(cancel_food_props_when_source_leaves)
        .add_observer(cancel_food_props_when_source_is_crated)
        .add_observer(link_food_prop_to_source)
        .add_observer(unlink_food_prop_from_source)
        .add_systems(FixedUpdate, (
        discard_retargeted_food_prop_attachments,
        update_waiting_behavior_actors.pipe(execute_food_prop_attachments),
    ).chain()
        .before(super::BehaviorObjectAttachmentSet)
        .in_set(BehaviorStepFixedSet::Think)
        .run_if(in_state(GamePhase::InGame)))
        .add_systems(FixedUpdate, receive_food_prop_animation_events
            .after(openzt2_zoo_state::behavior_task_execution_types::running_actions::BehaviorAnimationKeyDelivery)
            .in_set(openzt2_world_state::animation_playback::animation_event_message_types::AnimationObjectCommandDelivery)
            .in_set(FixedGameSet::Act)
            .run_if(in_state(GamePhase::InGame)));
}

fn discard_retargeted_food_prop_attachments(
    mut commands: Commands,
    actors: Query<
        (
            Entity,
            &HeldFoodPropAttachment,
            Option<&HeldFoodProp>,
            &BehaviorTaskExecutionState,
            &RunningBehaviorInstruction,
        ),
        Changed<BehaviorTaskExecutionState>,
    >,
) {
    for (actor, pending, prop, task, step) in &actors {
        if !pending.origin.owns(task, step) {
            cancel_food_prop(actor, prop, &mut commands);
        }
    }
}

fn cancel_food_prop(actor: Entity, prop: Option<&HeldFoodProp>, commands: &mut Commands) {
    if let Some(prop) = prop {
        commands.entity(*prop.collection()).try_despawn();
    }
    commands
        .entity(actor)
        .try_remove::<HeldFoodPropAttachment>();
}

fn cancel_removed_food_prop_instruction(
    removed: On<Remove<(RunBehaviorAttachObject, BehaviorTaskExecutionState)>>,
    actors: Query<Option<&HeldFoodProp>, With<HeldFoodPropAttachment>>,
    mut commands: Commands,
) {
    if let Ok(prop) = actors.get(removed.entity) {
        cancel_food_prop(removed.entity, prop, &mut commands);
    }
}

fn cancel_disabled_food_prop_instruction(
    added: On<
        Add<(
            PendingBehaviorTaskFailure,
            Dead,
            Frozen,
            Tranquilized,
            AnimalCrate,
            ScriptedCrating,
        )>,
    >,
    actors: Query<
        (Option<&HeldFoodProp>, Has<PendingBehaviorTaskFailure>),
        With<HeldFoodPropAttachment>,
    >,
    mut commands: Commands,
) {
    if let Ok((prop, failing)) = actors.get(added.entity) {
        cancel_food_prop(added.entity, prop, &mut commands);
        if !failing {
            mark_behavior_task_for_failure_and_stop_navigation(added.entity, &mut commands);
        }
    }
}

#[derive(Component)]
#[relationship(relationship_target = FoodPropsUsingSource)]
struct FoodPropSource(Entity);

#[derive(Component)]
#[relationship_target(relationship = FoodPropSource)]
struct FoodPropsUsingSource(Vec<Entity>);

fn link_food_prop_to_source(
    added: On<Add<HeldFoodPropAttachment>>,
    pending: Query<&HeldFoodPropAttachment>,
    mut commands: Commands,
) {
    if let Some(target) = pending
        .get(added.entity)
        .ok()
        .and_then(|pending| pending.origin.target)
    {
        commands
            .entity(added.entity)
            .try_insert(FoodPropSource(target));
    }
}

fn unlink_food_prop_from_source(
    removed: On<Remove<HeldFoodPropAttachment>>,
    mut commands: Commands,
) {
    commands
        .entity(removed.entity)
        .try_remove::<FoodPropSource>();
}

fn cancel_food_props_when_source_leaves(
    removed: On<Remove<WorldMember>>,
    sources: Query<&FoodPropsUsingSource>,
    actors: Query<Option<&HeldFoodProp>, With<HeldFoodPropAttachment>>,
    mut commands: Commands,
) {
    if let Ok(source) = sources.get(removed.entity) {
        cancel_food_props_for_source(source, &actors, &mut commands);
    }
}

fn cancel_food_props_when_source_is_crated(
    added: On<Add<(AnimalCrate, ScriptedCrating)>>,
    sources: Query<&FoodPropsUsingSource>,
    actors: Query<Option<&HeldFoodProp>, With<HeldFoodPropAttachment>>,
    mut commands: Commands,
) {
    if let Ok(source) = sources.get(added.entity) {
        cancel_food_props_for_source(source, &actors, &mut commands);
    }
}

fn cancel_food_props_for_source(
    source: &FoodPropsUsingSource,
    actors: &Query<Option<&HeldFoodProp>, With<HeldFoodPropAttachment>>,
    commands: &mut Commands,
) {
    for actor in source.iter() {
        if let Ok(prop) = actors.get(actor) {
            cancel_food_prop(actor, prop, commands);
            mark_behavior_task_for_failure_and_stop_navigation(actor, commands);
        }
    }
}

/// The temporary prop belongs to this instruction until its detach rule runs.
#[derive(Component)]
#[relationship(relationship_target = HeldFoodProp)]
struct FoodPropInstructionOwner(Entity);

#[derive(Component)]
#[relationship_target(relationship = FoodPropInstructionOwner, linked_spawn)]
struct HeldFoodProp(Entity);

pub(super) struct FoodPropAdmission {
    subject_joint: Option<Entity>,
    animation: Option<String>,
    tick: u64,
}

pub(super) fn finish_food_prop_admission(
    actor: Entity,
    item: Entity,
    admission: FoodPropAdmission,
    tasks: &mut Query<(&mut BehaviorTaskExecutionState, &RunningBehaviorInstruction)>,
    playback: &mut MessageWriter<AnimationClipPlaybackRequest>,
    commands: &mut Commands,
    pending: &mut Query<&mut HeldFoodPropAttachment>,
) {
    let Ok((mut task, instruction)) = tasks.get_mut(actor) else {
        commands.entity(item).try_despawn();
        return;
    };
    if !instruction.is_current(&task) {
        commands.entity(item).try_despawn();
        return;
    }
    let completed = admission.animation.is_none();
    let mut origin = PendingObjectAttachment::waiting_for_attachment(&task, instruction, None);
    origin.joint = admission.subject_joint;
    if let Some(animation_clip_asset_key) = admission.animation {
        origin.request_id = Some(
            playback
                .write(AnimationClipPlaybackRequest {
                    animation_subject_entity: actor,
                    animation_clip_asset_key,
                    blend_duration_milliseconds: 0,
                    playback_speed_permille: 1000,
                    playback_repetition_policy: AnimationPlaybackRepetitionPolicy::PlayOnce,
                    action_settings: None,
                })
                .id,
        );
    }
    commands
        .entity(item)
        .try_insert(FoodPropInstructionOwner(actor));
    let attachment = HeldFoodPropAttachment {
        origin,
        attached: completed,
        detached: false,
        completed,
    };
    if let Ok(mut pending) = pending.get_mut(actor) {
        *pending = attachment;
    } else {
        commands.entity(actor).try_insert(attachment);
    }
    task.next_action_tick = admission.tick.saturating_add(1);
}

fn execute_food_prop_attachments(
    activation: In<Option<Entity>>,
    mut commands: Commands,
    assets: AttachmentAssets,
    mut prefab_events: AttachmentPrefabEvents,
    loaded: Res<LoadedBehaviorDocumentCollection>,
    animal_types: AnimalBehaviorSubjectTypes,
    clock: Single<&ZooClock>,
    mut requests: MessageWriter<AnimationClipPlaybackRequest>,
    mut actors: Query<
        (
            Entity,
            (
                &mut BehaviorTaskExecutionState,
                &RunningBehaviorInstruction,
                &RunBehaviorAttachObject,
            ),
            &DefinitionId,
            &WorldMember,
            Option<&mut HeldFoodPropAttachment>,
            Option<&HeldFoodProp>,
            Option<&SpeciesHandle>,
            Option<&StaffRole>,
            Option<&GuestArchetype>,
            Has<WaitingForAttachmentDefinitions>,
        ),
        (
            Without<PendingBehaviorTaskFailure>,
            Without<Dead>,
            Without<Frozen>,
            Without<Tranquilized>,
            UncratedWorldEntity,
        ),
    >,
    sources: Query<(&DefinitionId, &WorldMember), UncratedWorldEntity>,
    objects: Query<(&ContainedObject, &DefinitionId, Option<&WorldMember>)>,
    transforms: Query<&GlobalTransform>,
    nodes: super::super::behavior_named_nodes::NamedNodeTransforms,
) {
    let settled = prefab_events.settled();
    let definitions_changed = prefab_events.definitions_changed(&assets.definitions);
    let Some(definitions) = assets.definitions.get(&assets.definition_assets) else {
        return;
    };
    for (
        actor,
        (mut task, instruction, running_action),
        actor_definition,
        world,
        mut pending,
        prop,
        species,
        staff,
        guest,
        waiting,
    ) in behavior_action_actors(&mut actors, activation.0)
    {
        if let Some(pending) = pending.as_mut() {
            if !pending.origin.prefab_settled {
                if let Some(prefab) = &pending.origin.prefab {
                    if !settled.contains(&prefab.id()) {
                        continue;
                    }
                    pending.origin.prefab_settled = true;
                }
            }
        }
        if activation.0.is_none() && !instruction.can_update_after_activation(clock.tick) {
            continue;
        }
        if task.next_action_tick > clock.tick {
            continue;
        }
        if !instruction.is_current(&task) {
            continue;
        }
        let action = &running_action.0;
        if !action.attaches_target_held_prop() {
            continue;
        }
        if waiting && activation.0.is_none() && !definitions_changed {
            continue;
        }
        if let Some(pending) = pending
            .as_ref()
            .filter(|pending| pending.origin.request_id.is_some() || pending.completed)
        {
            if !pending.detached && !pending.completed {
                continue;
            }
            if prop.is_some() && !pending.attached {
                mark_behavior_task_for_failure_and_stop_navigation(actor, &mut commands);
                continue;
            }
            let continuation = if let Some(set) = action.detach_behavior_set {
                let Some(view) = loaded.create_declaration_index_view(&assets.documents) else {
                    continue;
                };
                let Some(location) = view.find_behavior_set_location(
                    set,
                    behavior_subject_type_identifiers(
                        species,
                        staff,
                        guest,
                        definitions,
                        animal_types.borrow_for(actor),
                    ),
                ) else {
                    mark_behavior_task_for_failure_and_stop_navigation(actor, &mut commands);
                    continue;
                };
                Some((set, location))
            } else {
                None
            };
            if let Some(item) = prop.map(|prop| *prop.collection()) {
                if !super::detach::detach_object(
                    actor,
                    item,
                    None,
                    &definitions,
                    &objects,
                    &transforms,
                    &mut commands,
                ) {
                    mark_behavior_task_for_failure_and_stop_navigation(actor, &mut commands);
                    continue;
                }
                commands
                    .entity(item)
                    .try_remove::<FoodPropInstructionOwner>();
            }
            commands.entity(actor).remove::<HeldFoodPropAttachment>();
            if let Some((set, (document, declaration))) = continuation {
                instruction.enter_set(
                    actor,
                    SelectedBehaviorSet {
                        program: set,
                        document,
                        declaration,
                        tick: clock.tick,
                        repeat: None,
                        target: None,
                    },
                    &mut commands,
                );
            } else {
                instruction.complete(actor, clock.tick, &mut commands);
            }
            continue;
        }
        let Some(target) = task.target else {
            mark_behavior_task_for_failure_and_stop_navigation(actor, &mut commands);
            continue;
        };
        let Some(source) = sources
            .get(target)
            .ok()
            .filter(|(_, source_world)| source_world.root == world.root)
            .and_then(|(id, _)| definitions.find_object(id.0))
            .and_then(|source| source.container_quantity)
        else {
            wait_for_attachment_definition(actor, waiting, &assets.definitions, &mut commands);
            continue;
        };
        let mut state = HeldFoodPropAttachment {
            origin: PendingObjectAttachment::waiting_for_attachment(&task, instruction, None),
            attached: source.held_prop.is_none(),
            detached: false,
            completed: action.animation.is_none(),
        };
        if let Some(name) = &action.subject_node {
            let identifier = AssetId::from_key(&name.to_ascii_lowercase());
            let Some(joint) = nodes.socket(actor, identifier) else {
                mark_behavior_task_for_failure_and_stop_navigation(actor, &mut commands);
                continue;
            };
            state.origin.joint = Some(joint);
        }
        if let Some(prop) = source.held_prop {
            let Some(object) = definitions.find_object(prop) else {
                wait_for_attachment_definition(actor, waiting, &assets.definitions, &mut commands);
                continue;
            };
            let Some((slot, place)) =
                holder::AttachmentHolder::find(&definitions, actor_definition.0).and_then(
                    |holder| {
                        holder
                            .place_for(action.container)
                            .and_then(|slot| Some((slot, holder.place(slot)?)))
                    },
                )
            else {
                mark_behavior_task_for_failure_and_stop_navigation(actor, &mut commands);
                continue;
            };
            let joint = if let Some(name) = &action.target_node {
                let identifier = AssetId::from_key(&name.to_ascii_lowercase());
                let Some(joint) = nodes.socket(target, identifier) else {
                    mark_behavior_task_for_failure_and_stop_navigation(actor, &mut commands);
                    continue;
                };
                joint
            } else {
                target
            };
            let Ok(transform) = transforms.get(joint) else {
                continue;
            };
            let Some(handle) = pending
                .as_ref()
                .and_then(|pending| pending.origin.prefab.clone())
                .or_else(|| definitions.scene(object.prefab))
            else {
                wait_for_attachment_definition(actor, waiting, &assets.definitions, &mut commands);
                continue;
            };
            if waiting {
                commands
                    .entity(actor)
                    .try_remove::<WaitingForAttachmentDefinitions>();
            }
            state.origin.prefab = Some(handle.clone());
            let Some(prefab) = assets.prefabs.get(&handle) else {
                if matches!(
                    assets.server.get_load_state(handle.id()),
                    Some(LoadState::Failed(_))
                ) {
                    mark_behavior_task_for_failure_and_stop_navigation(actor, &mut commands);
                } else if pending.is_none() {
                    commands.entity(actor).insert(state);
                }
                continue;
            };
            let Some(root) = prefab.canonical_scene_prefab_document().entities.first() else {
                mark_behavior_task_for_failure_and_stop_navigation(actor, &mut commands);
                continue;
            };
            let scale = Transform::from_scale(Vec3::splat(object.prefab_scale));
            let definition = object.id;
            let instance_transform = transform.compute_transform().mul_transform(scale);
            let attachment = ContainedObject {
                holder_definition: actor_definition.0,
                detach_rule: action.detach_rule,
                joint: Some(joint),
                relative_transform: scale.mul_transform(transform_from_authored(&root.transform)),
                attachment_request: None,
            };
            let modifier = action.locomotion_modifier;
            let animation = action.animation.clone();
            let tick = clock.tick;
            admission::request_new_object_admission(
                &mut commands,
                actor,
                slot,
                place.capacity,
                world.root,
                admission::AttachmentAdmission::Attach(admission::AttachAfterAdmission {
                    actor,
                    instruction: BehaviorAdmissionInstruction::new(actor, &task, instruction),
                    attachment,
                    visibility: Visibility::Inherited,
                    modifier,
                    spawn: Some(admission::NewAttachedObject {
                        world: *world,
                        definition,
                        instance_transform,
                        prefab: Some(handle),
                    }),
                    finish: admission::AttachmentAfterAdmission::FoodProp(FoodPropAdmission {
                        subject_joint: state.origin.joint,
                        animation,
                        tick,
                    }),
                }),
            );
            continue;
        }
        if waiting {
            commands
                .entity(actor)
                .try_remove::<WaitingForAttachmentDefinitions>();
        }
        if let Some(animation) = &action.animation {
            state.origin.request_id = Some(
                requests
                    .write(AnimationClipPlaybackRequest {
                        animation_subject_entity: actor,
                        animation_clip_asset_key: animation.clone(),
                        blend_duration_milliseconds: 0,
                        playback_speed_permille: 1000,
                        playback_repetition_policy: AnimationPlaybackRepetitionPolicy::PlayOnce,
                        action_settings: None,
                    })
                    .id,
            );
        }
        if let Some(mut pending) = pending {
            *pending = state;
        } else {
            commands.entity(actor).insert(state);
        }
        task.next_action_tick = clock.tick.saturating_add(1);
    }
}

fn receive_food_prop_animation_events(
    mut commands: Commands,
    mut events: MessageReader<AnimationObjectCommand>,
    mut completed: MessageReader<AnimationCompleted>,
    mut rejected: MessageReader<AnimationClipPlaybackRequestRejected>,
    owners: Query<(&AnimationPresentationOwner, &AnimationPlaybackController)>,
    mut actors: Query<
        (
            &BehaviorTaskExecutionState,
            &RunningBehaviorInstruction,
            &mut HeldFoodPropAttachment,
            Option<&HeldFoodProp>,
        ),
        Without<PendingBehaviorTaskFailure>,
    >,
    mut props: Query<&mut ContainedObject>,
) {
    let mut failed = bevy::platform::collections::HashSet::new();
    for rejection in rejected.read() {
        let Ok((task, step, pending, _)) = actors.get(rejection.animation_subject_entity) else {
            continue;
        };
        if pending.origin.request_id == Some(rejection.request_id)
            && pending.origin.owns(task, step)
        {
            failed.insert(rejection.request_id);
            mark_behavior_task_for_failure_and_stop_navigation(
                rejection.animation_subject_entity,
                &mut commands,
            );
        }
    }
    for event in events.read() {
        let Ok((owner, playback)) = owners.get(event.controller) else {
            continue;
        };
        let Ok((task, step, mut pending, prop)) = actors.get_mut(owner.gameplay_entity) else {
            continue;
        };
        if pending.origin.request_id.is_none()
            || event.playback_generation != playback.playback_generation
            || event.request_id != playback.explicit_clip_request_id
            || event.request_id != pending.origin.request_id
            || event.request_id.is_some_and(|id| failed.contains(&id))
            || !pending.origin.owns(task, step)
        {
            continue;
        }
        match &event.action {
            AuthoredAnimationTextAction::AttachPendingObject
            | AuthoredAnimationTextAction::AttachNamedObject { .. } => {
                if let Some(item) = prop.map(|prop| *prop.collection()) {
                    if let Ok(mut prop) = props.get_mut(item) {
                        prop.joint = Some(pending.origin.joint.unwrap_or(event.joint));
                        pending.attached = true;
                    }
                }
            }
            AuthoredAnimationTextAction::DetachObject if pending.attached => {
                pending.detached = true;
            }
            _ => {}
        }
    }
    for event in completed.read() {
        let Ok((owner, playback)) = owners.get(event.animation_playback_controller_entity) else {
            continue;
        };
        let Ok((task, step, mut pending, _)) = actors.get_mut(owner.gameplay_entity) else {
            continue;
        };
        if pending.origin.request_id.is_some()
            && event.playback_generation == playback.playback_generation
            && event.explicit_clip_request_id == pending.origin.request_id
            && !event
                .explicit_clip_request_id
                .is_some_and(|id| failed.contains(&id))
            && pending.origin.owns(task, step)
        {
            pending.completed = true;
        }
    }
}
