//! Tools a person holds only while an animation calls for them, such as a
//! worker's broom: `{Bip01 R Finger0} create Broom` in `Stand_2Sweep` puts the
//! actor's `Broom` binder in its hand, and `destroy Broom` in `Sweep_2Stand`
//! takes it away.

use bevy::prelude::*;
use openzt2_zoo_state::behavior_task_execution_types::BehaviorTaskExecutionState;
use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use openzt2_game_assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use openzt2_game_assets::animation::animation_set_asset_types::AnimationSetAsset;
use openzt2_game_data::animation::animation_text_key::AuthoredAnimationTextAction;
use openzt2_world_state::animation_playback::animation_joint_target_types::{
    AnimationJointTarget, HeldPropPresentation,
};
use openzt2_game_data::AssetId;
use openzt2_world_state::animation_graph::animation_presentation_relationship_types::AnimationPresentationOwner;
use openzt2_world_state::animation_playback::animation_event_message_types::AnimationObjectCommand;
use openzt2_world_state::world_spawn::prefab_presentation_types::PrefabPresentation;
use openzt2_world_state::world_spawn::world_membership_types::DefinitionId;

/// An explicit clip entered `to` straight from `from` without playing the
/// transition clips between them, such as `Stand_2Sweep` on the way into a
/// worker's `Sweep`.
#[derive(Message, Clone, Debug)]
pub(crate) struct SkippedAnimationGraphTransition {
    pub(crate) controller: Entity,
    pub(crate) animation_set: Handle<AnimationSetAsset>,
    pub(crate) from: String,
    pub(crate) to: String,
}

/// The transition clips on the shortest authored route from `from` to `to`.
fn transition_clips_between<'a>(set: &'a AnimationSetAsset, from: &str, to: &str) -> Vec<&'a str> {
    let mut previous = std::collections::HashMap::<&str, (&str, usize)>::new();
    let mut queue = std::collections::VecDeque::from([from]);
    while let Some(node) = queue.pop_front() {
        if node.eq_ignore_ascii_case(to) {
            let mut clips = Vec::new();
            let mut at = node;
            while let Some(&(source, edge)) = previous.get(at) {
                clips.splice(
                    0..0,
                    set.animation_graph_edges[edge]
                        .transition_animation_clip_asset_keys
                        .iter()
                        .map(String::as_str),
                );
                at = source;
            }
            return clips;
        }
        for (index, edge) in set.animation_graph_edges.iter().enumerate() {
            let target = edge.target_animation_graph_node_asset_key.as_str();
            if edge
                .source_animation_graph_node_asset_key
                .eq_ignore_ascii_case(node)
                && !target.eq_ignore_ascii_case(from)
                && !previous.contains_key(target)
            {
                previous.insert(target, (node, index));
                queue.push_back(target);
            }
        }
    }
    Vec::new()
}

/// A binder an animation put in a joint, by the controller that created it.
#[derive(Component)]
pub(super) struct AnimationHeldProp {
    controller: Entity,
    name: AssetId,
    /// The actor and the behavior task that was running when the prop
    /// appeared. An interrupted task never plays the clip that puts the prop
    /// away, so the prop goes with the task.
    task: Option<(Entity, u64)>,
}

pub(super) fn create_and_destroy_animation_held_props(
    mut commands: Commands,
    mut events: MessageReader<AnimationObjectCommand>,
    active_definitions: Res<WorldDefinitions>,
    definition_assets: Res<Assets<WorldDefinitionAsset>>,
    identities: Query<(Option<&DefinitionId>, Option<&AnimationPresentationOwner>)>,
    parents: Query<&ChildOf>,
    props: Query<(Entity, &AnimationHeldProp)>,
    mut skipped: MessageReader<SkippedAnimationGraphTransition>,
    mut waiting: Local<Vec<(SkippedAnimationGraphTransition, u32)>>,
    mut animation_sets: ResMut<Assets<AnimationSetAsset>>,
    asset_server: Res<AssetServer>,
    joints: Query<(Entity, &AnimationJointTarget)>,
    tasks: Query<&BehaviorTaskExecutionState>,
) {
    let Some(definitions) = active_definitions.get(&definition_assets) else {
        events.clear();
        skipped.clear();
        return;
    };
    // Each skipped transition clip's create and destroy keys, in order. A
    // transition clip that never played isn't loaded yet; its transition
    // waits for the load, and later ones of the same controller wait behind
    // it so a destroy can't overtake its create.
    waiting.extend(skipped.read().map(|transition| (transition.clone(), 0)));
    let mut skipped_commands = Vec::new();
    let mut blocked = Vec::new();
    let mut still_waiting = Vec::new();
    for (transition, attempts) in std::mem::take(&mut *waiting) {
        if blocked.contains(&transition.controller) {
            still_waiting.push((transition, attempts));
            continue;
        }
        let Some(set) = animation_sets.get(&transition.animation_set) else {
            continue;
        };
        let clips = transition_clips_between(set, &transition.from, &transition.to)
            .into_iter()
            .map(str::to_owned)
            .collect::<Vec<_>>();
        let loading = clips
            .iter()
            .filter(|clip| {
                set.find_animation_clip_record_by_asset_key(clip).is_none()
                    && !set.animation_clip_asset_load_has_failed(clip)
                    && set
                        .find_animation_clip_source_reference_by_asset_key(clip)
                        .is_some()
            })
            .cloned()
            .collect::<Vec<_>>();
        // Give up after about ten seconds of fixed updates.
        if !loading.is_empty() && attempts < 600 {
            if let Some(mut set) = animation_sets.get_mut(&transition.animation_set) {
                for clip in &loading {
                    if !set.animation_clip_asset_load_is_pending(clip) {
                        set.request_animation_clip_asset_load(&asset_server, clip);
                    }
                }
            }
            blocked.push(transition.controller);
            still_waiting.push((transition, attempts + 1));
            continue;
        }
        let Some(set) = animation_sets.get(&transition.animation_set) else {
            continue;
        };
        for clip in clips.iter().map(String::as_str) {
            let Some(metadata) = set
                .find_animation_clip_record_by_asset_key(clip)
                .and_then(|record| record.authored_animation_clip_metadata.as_ref())
            else {
                continue;
            };
            for command in metadata
                .authored_animation_text_keys
                .iter()
                .flat_map(|key| key.animation_text_commands.iter())
            {
                if !matches!(
                    command.animation_text_action,
                    AuthoredAnimationTextAction::CreateObject { .. }
                        | AuthoredAnimationTextAction::CreateObjectOverride { .. }
                        | AuthoredAnimationTextAction::DestroyObject { .. }
                ) {
                    continue;
                }
                let joint_key = command
                    .target_skeleton_joint_asset_key
                    .as_deref()
                    .unwrap_or("");
                let joint = joints
                    .iter()
                    .find_map(|(entity, joint)| {
                        (joint.animation_playback_controller_entity == transition.controller
                            && joint.joint_asset_key.eq_ignore_ascii_case(joint_key))
                        .then_some(entity)
                    })
                    .unwrap_or(transition.controller);
                skipped_commands.push((
                    transition.controller,
                    joint,
                    command.animation_text_action.clone(),
                ));
            }
        }
    }
    *waiting = still_waiting;
    let commands_from_events = events
        .read()
        .map(|event| (event.controller, event.joint, event.action.clone()))
        .collect::<Vec<_>>();
    for (controller, joint, action) in skipped_commands.into_iter().chain(commands_from_events) {
        let event = HeldPropCommand {
            controller,
            joint,
            action,
        };
        let (key, create) = match &event.action {
            AuthoredAnimationTextAction::CreateObject { object_asset_key }
            | AuthoredAnimationTextAction::CreateObjectOverride { object_asset_key } => {
                (object_asset_key, true)
            }
            AuthoredAnimationTextAction::DestroyObject { object_asset_key } => {
                (object_asset_key, false)
            }
            _ => continue,
        };
        let name = AssetId::from_key(&key.trim().to_ascii_lowercase());
        let held = props
            .iter()
            .filter(|(_, prop)| prop.controller == event.controller && prop.name == name)
            .map(|(entity, _)| entity)
            .collect::<Vec<_>>();
        if !create {
            for prop in held {
                commands.entity(prop).despawn();
            }
            continue;
        }
        if !held.is_empty() {
            continue;
        }
        let definition = std::iter::once(event.controller)
            .chain(parents.iter_ancestors(event.controller))
            .find_map(|entity| {
                let (definition, owner) = identities.get(entity).ok()?;
                definition
                    .copied()
                    .or_else(|| identities.get(owner?.gameplay_entity).ok()?.0.copied())
            });
        let Some((binder, prefab)) = definition
            .and_then(|definition| definitions.find_object(definition.0))
            .and_then(|object| {
                object
                    .named_physical_presentations
                    .iter()
                    .find(|binder| binder.name == name)
            })
            .and_then(|binder| Some((binder, definitions.scene(binder.prefab)?)))
        else {
            debug!(controller = ?event.controller, ?definition, object = %key,
                "animation creates an object its actor has no binder for");
            continue;
        };
        let task = std::iter::once(event.controller)
            .chain(parents.iter_ancestors(event.controller))
            .find_map(|entity| identities.get(entity).ok()?.1)
            .and_then(|owner| {
                let task = tasks.get(owner.gameplay_entity).ok()?;
                Some((owner.gameplay_entity, task.execution_id))
            });
        debug!(controller = ?event.controller, object = %key, "animation shows a held object");
        commands.spawn((
            Transform::from_scale(Vec3::splat(binder.scale)),
            Visibility::Inherited,
            ChildOf(event.joint),
            PrefabPresentation::new(prefab),
            HeldPropPresentation,
            AnimationHeldProp {
                controller: event.controller,
                name,
                task,
            },
        ));
    }
}

/// Takes away a prop whose task ended or was replaced before its animation
/// put it away, as when a worker sweeping is interrupted.
pub(super) fn release_held_props_of_interrupted_tasks(
    mut commands: Commands,
    props: Query<(Entity, &AnimationHeldProp)>,
    tasks: Query<&BehaviorTaskExecutionState>,
) {
    for (entity, prop) in &props {
        let Some((actor, execution)) = prop.task else {
            continue;
        };
        if tasks.get(actor).map(|task| task.execution_id).ok() != Some(execution) {
            commands.entity(entity).despawn();
        }
    }
}

struct HeldPropCommand {
    controller: Entity,
    joint: Entity,
    action: AuthoredAnimationTextAction,
}

#[cfg(test)]
mod tests {
    use super::*;
    use openzt2_game_data::animation::animation_set::AuthoredAnimationGraphEdge;

    fn edge(from: &str, to: &str, clip: &str) -> AuthoredAnimationGraphEdge {
        AuthoredAnimationGraphEdge {
            source_animation_graph_node_asset_key: from.to_owned(),
            target_animation_graph_node_asset_key: to.to_owned(),
            transition_animation_clip_asset_keys: vec![clip.to_owned()],
            authored_animation_attributes: Vec::new(),
        }
    }

    #[test]
    fn a_skipped_route_lists_its_transition_clips_in_order() {
        let set = AnimationSetAsset {
            model_asset_path: String::new(),
            skeleton_asset_path: String::new(),
            animation_clips: Vec::new(),
            animation_graph_nodes: Vec::new(),
            animation_graph_order: Vec::new(),
            animation_graph_edges: vec![
                edge("Walk", "Stand", "Walk_2Stand"),
                edge("Stand", "Sweep", "Stand_2Sweep"),
                edge("Sweep", "Stand", "Sweep_2Stand"),
                edge("Stand", "Walk", "Stand_2Walk"),
            ],
            animation_graph_metadata: None,
            animation_clip_source_references: Vec::new(),
            pending_animation_clip_source_asset_handles: Vec::new(),
            failed_animation_clip_asset_keys: Vec::new(),
        };
        assert_eq!(
            transition_clips_between(&set, "Walk", "Sweep"),
            ["Walk_2Stand", "Stand_2Sweep"]
        );
        assert_eq!(
            transition_clips_between(&set, "Sweep", "Walk"),
            ["Sweep_2Stand", "Stand_2Walk"]
        );
        assert!(transition_clips_between(&set, "Sweep", "Swim").is_empty());
    }

    #[test]
    fn a_prop_goes_with_its_interrupted_task() {
        use openzt2_zoo_state::behavior_task_execution_types::BehaviorTaskExecutionState;
        let mut world = World::new();
        let actor = world.spawn_empty().id();
        let held = |world: &mut World, execution| {
            world
                .spawn(AnimationHeldProp {
                    controller: Entity::PLACEHOLDER,
                    name: AssetId::default(),
                    task: Some((actor, execution)),
                })
                .id()
        };
        let current = held(&mut world, 1);
        let stale = held(&mut world, 0);
        world.entity_mut(actor).insert(BehaviorTaskExecutionState {
            execution_id: 1,
            program: AssetId::default(),
            target: None,
            object: None,
            document: Default::default(),
            declaration: 0,
            phase:
                openzt2_zoo_state::behavior_task_execution_types::BehaviorTaskExecutionPhase::Set,
            action: 0,
            repetitions: 0,
            next_action_tick: 0,
            interaction_slot: None,
            stack: Default::default(),
        });
        let mut system = IntoSystem::into_system(release_held_props_of_interrupted_tasks);
        system.initialize(&mut world);
        system.run((), &mut world).unwrap();
        world.flush();
        assert!(world.get_entity(current).is_ok());
        assert!(world.get_entity(stale).is_err());
    }
}
