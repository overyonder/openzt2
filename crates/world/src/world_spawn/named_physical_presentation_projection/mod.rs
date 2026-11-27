use super::prefab_socket_index::PrefabSocketIndex;
use super::prefab_attachment_independent_rotation::PrefabAttachmentIndependentRotation;
use openzt2_world_state::world_spawn::prefab_authored_attachment_identifier::PrefabAuthoredAttachmentIdentifier;
use openzt2_world_state::world_spawn::prefab_presentation_types::PrefabPresentation;
use openzt2_world_state::world_spawn::prefab_presentation_types::PrefabCausticPolicy;
use openzt2_world_state::world_spawn::world_membership_types::DefinitionId;
use openzt2_world_state::animation_graph::animation_presentation_relationship_types::{AnimationPresentationBinder, AnimationPresentationOwner};
use bevy::prelude::*;
use openzt2_world_state::animation_playback::traversal_animation_path_types::PrefabTraversalAnimationManifests;
use bevy::platform::collections::HashSet;
use openzt2_game_assets::scene_prefab::ScenePrefabAsset;
use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use openzt2_game_assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use openzt2_game_data::world_definitions::world_objects::{
    WorldObjectFadeRange, WorldObjectNamedPhysicalPresentationDefinition,
};
use bevy::app::Propagate;
use openzt2_zoo_state::authored_float_attributes::AuthoredFloatAttributes;
use std::{collections::BTreeMap, time::Duration};

use super::prefab_world_instance_spawning::morph_fade_projection::NamedPrefabOpacity;
use openzt2_game_data::AssetId;

pub(super) mod animation_key_commands;

#[cfg(test)]
mod node_attachment_tests;
#[cfg(test)]
mod tests;

#[derive(Component)]
pub(super) struct NamedPhysicalPresentationsHydrated;

/// A named physical object retaining its owner after attachment to a model node.
#[derive(Component)]
#[relationship(relationship_target = NamedPhysicalPresentations)]
pub(super) struct NamedPhysicalPresentation(Entity);

#[derive(Component)]
#[relationship_target(relationship = NamedPhysicalPresentation, linked_spawn)]
pub(super) struct NamedPhysicalPresentations(Vec<Entity>);

/// Borrows each physical binder's policy without applying it to sibling binders.
pub(super) fn project_physical_presentation_caustic_policies(
    mut commands: Commands,
    active_definitions: Res<WorldDefinitions>,
    assets: Res<Assets<WorldDefinitionAsset>>,
    mut asset_events: MessageReader<AssetEvent<WorldDefinitionAsset>>,
    server: Option<Res<AssetServer>>,
    mut presentations: Query<
        (
            Entity,
            Option<&DefinitionId>,
            Option<&PrefabAuthoredAttachmentIdentifier>,
            Option<&mut PrefabCausticPolicy>,
            Option<&mut PrefabTraversalAnimationManifests>,
        ),
        Or<(
            With<DefinitionId>,
            With<PrefabPresentation>,
            With<PrefabCausticPolicy>,
            With<PrefabTraversalAnimationManifests>,
        )>,
    >,
    parents: Query<&ChildOf>,
    children: Query<&Children>,
    definitions: Query<&DefinitionId>,
    changed_hierarchy: Query<Entity, Or<(Changed<DefinitionId>, Changed<ChildOf>)>>,
    changed_presentations: Query<
        Entity,
        Or<(
            Changed<PrefabAuthoredAttachmentIdentifier>,
            Changed<PrefabPresentation>,
        )>,
    >,
    mut removed_definitions: RemovedComponents<DefinitionId>,
    mut removed_parents: RemovedComponents<ChildOf>,
    mut removed_binder_ids: RemovedComponents<PrefabAuthoredAttachmentIdentifier>,
    mut removed_presentations: RemovedComponents<PrefabPresentation>,
) {
    let catalogue_changed = asset_events.read().count() != 0 || active_definitions.is_changed();
    let hierarchy_changes = changed_hierarchy
        .iter()
        .chain(removed_definitions.read())
        .chain(removed_parents.read());
    let local_changes = changed_presentations
        .iter()
        .chain(removed_binder_ids.read())
        .chain(removed_presentations.read());
    let mut affected = HashSet::<Entity>::default();
    if catalogue_changed {
        affected.extend(presentations.iter().map(|(entity, ..)| entity));
    }
    for entity in hierarchy_changes {
        if affected.insert(entity) {
            affected.extend(children.iter_descendants::<Children>(entity));
        }
    }
    affected.extend(local_changes);
    if affected.is_empty() {
        return;
    }
    let catalogue = active_definitions.get(&assets);
    let mut changed = presentations.iter_many_mut(affected).matched();
    while let Some((entity, own_definition, binder, current, manifests)) = changed.fetch_next() {
        let definition = own_definition.or_else(|| {
            parents
                .iter_ancestors::<ChildOf>(entity)
                .find_map(|ancestor| definitions.get(ancestor).ok())
        });
        let binder = if own_definition.is_some() {
            Some(AssetId::from_key("mainobj"))
        } else {
            binder.map(|binder| binder.0)
        };
        let object_binder = definition
            .zip(binder)
            .and_then(|(id, binder)| Some((catalogue.as_ref()?.find_object(id.0)?, binder)));
        let projected = object_binder.and_then(|(definition, binder)| {
            let policy = definition
                .presentation_caustic_policies
                .iter()
                .find(|policy| policy.binder == binder)?;
            Some(PrefabCausticPolicy {
                receives: !policy.no_caustics && !policy.is_decal,
            })
        });
        if let Some(server) = server.as_ref() {
            let sources = object_binder
                .into_iter()
                .flat_map(|(definition, binder)| {
                    definition
                        .traversal_animation_manifests
                        .iter()
                        .filter(move |manifest| manifest.binder == binder)
                })
                .map(|manifest| server.load(manifest.source_path.clone()))
                .collect::<Box<[_]>>();
            match (manifests, sources.is_empty()) {
                (Some(mut manifests), false) => {
                    manifests.set_if_neq(PrefabTraversalAnimationManifests(sources));
                }
                (None, false) => {
                    commands
                        .entity(entity)
                        .try_insert(PrefabTraversalAnimationManifests(sources));
                }
                (Some(_), true) => {
                    commands
                        .entity(entity)
                        .try_remove::<PrefabTraversalAnimationManifests>();
                }
                (None, true) => {}
            }
        }
        match (current, projected) {
            (Some(mut current), Some(projected)) => {
                current.set_if_neq(projected);
            }
            (Some(_), None) => {
                commands.entity(entity).try_remove::<PrefabCausticPolicy>();
            }
            (None, Some(policy)) => {
                commands.entity(entity).try_insert(policy);
            }
            (None, None) => {}
        }
    }
}

pub(super) fn hydrate_required_named_physical_presentations(
    mut commands: Commands,
    active_definitions: Res<WorldDefinitions>,
    assets: Res<Assets<WorldDefinitionAsset>>,
    owners: Query<(Entity, &DefinitionId), Without<NamedPhysicalPresentationsHydrated>>,
) {
    let Some(definitions) = active_definitions.get(&assets) else {
        return;
    };
    for (owner, definition) in &owners {
        let Some(object) = definitions.find_object(definition.0) else {
            continue;
        };
        // An unregistered scene never appears later. Skip that binder with a
        // warning instead of retrying the whole object every frame.
        let presentations = object
            .named_physical_presentations
            .iter()
            .filter(|presentation| presentation.required)
            .filter_map(|presentation| {
                if presentation.prefab == AssetId::default() {
                    return Some((presentation, None));
                }
                let Some(prefab) = definitions.scene(presentation.prefab) else {
                    warn!(
                        definition = ?definition.0,
                        binder = ?presentation.name,
                        "named physical presentation has no registered scene; spawning the object without it"
                    );
                    return None;
                };
                Some((presentation, Some(prefab)))
            })
            .collect::<Vec<_>>();
        spawn_required_named_physical_presentations(&mut commands, owner, presentations);
    }
}

pub(super) fn spawn_required_named_physical_presentations<'a>(
    commands: &mut Commands,
    owner: Entity,
    presentations: impl IntoIterator<
        Item = (
            &'a WorldObjectNamedPhysicalPresentationDefinition,
            Option<Handle<ScenePrefabAsset>>,
        ),
    >,
) {
    for (presentation, prefab) in presentations {
        // A node-attached binder hangs from a holder that is later reparented
        // onto its node; the binder's own scale stays on the child.
        let parent = presentation.attach_node.as_ref().map_or(owner, |node| {
            let mut holder = commands.spawn((
                Transform::IDENTITY,
                Visibility::Hidden,
                ChildOf(owner),
                NamedPresentationNodeAttachment {
                    owner,
                    node: node.clone(),
                },
            ));
            if !presentation.inherit_attach_rotation {
                holder.insert(PrefabAttachmentIndependentRotation);
            }
            holder.id()
        });
        let mut child = commands.spawn((
            Transform::from_scale(Vec3::splat(presentation.scale)),
            if presentation.initially_visible {
                Visibility::Inherited
            } else {
                Visibility::Hidden
            },
            ChildOf(parent),
            PrefabAuthoredAttachmentIdentifier(presentation.name),
            AnimationPresentationBinder(presentation.name),
            NamedPhysicalPresentation(owner),
        ));
        if let Some(prefab) = prefab {
            child.insert(PrefabPresentation::new(prefab));
        }
        if let Some(fade) = presentation.fade {
            child.insert((
                NamedPresentationFade {
                    range: fade,
                    selected: presentation.initially_visible,
                    accepted_value: None,
                },
                Propagate(NamedPrefabOpacity(if presentation.initially_visible {
                    fade.max_alpha
                } else {
                    0.0
                })),
            ));
        }
    }
    commands
        .entity(owner)
        .insert_if_new(NamedPhysicalPresentationsHydrated);
}

#[derive(Component)]
pub(super) struct NamedPresentationFade {
    range: WorldObjectFadeRange,
    selected: bool,
    accepted_value: Option<f32>,
}

#[derive(Component)]
pub(super) struct NamedPresentationOpacityTransition {
    timer: Timer,
    from: f32,
    to: f32,
}

type NamedFadeBinder = (
    Entity,
    &'static mut NamedPresentationFade,
    &'static mut Visibility,
    &'static mut Propagate<NamedPrefabOpacity>,
    Option<&'static mut NamedPresentationOpacityTransition>,
);

pub(super) fn seed_named_presentation_fade_inputs(
    mut commands: Commands,
    catalogue: Res<WorldDefinitions>,
    assets: Res<Assets<WorldDefinitionAsset>>,
    added: Query<&NamedPhysicalPresentation, Added<NamedPresentationFade>>,
    fades: Query<&NamedPresentationFade>,
    mut owners: Query<(
        Entity,
        &DefinitionId,
        &NamedPhysicalPresentations,
        Option<&mut AuthoredFloatAttributes>,
    )>,
) {
    let Some(definitions) = catalogue.get(&assets) else {
        return;
    };
    let owners_with_new_fades = added
        .iter()
        .map(|presentation| presentation.0)
        .collect::<bevy::ecs::entity::EntityHashSet>();
    let mut pending = owners.iter_many_mut(owners_with_new_fades).matched();
    while let Some((owner, definition, children, existing)) = pending.fetch_next() {
        let Some(object) = definitions.find_object(definition.0) else {
            continue;
        };
        let properties = &object.authored_runtime.numeric_behavior_properties;
        let mut initial = AuthoredFloatAttributes::default();
        let mut existing = existing;
        for fade in fades.iter_many(&children.0).matched() {
            let attribute = fade.range.attribute;
            let Some((_, value)) = properties
                .iter()
                .find(|(key, value)| *key == attribute && value.is_finite())
            else {
                continue;
            };
            if let Some(values) = existing.as_mut() {
                if values.get(attribute).is_none() {
                    values.set(attribute, *value);
                }
            } else {
                initial.set(attribute, *value);
            }
        }
        if initial.iter().len() != 0 {
            commands.entity(owner).try_insert_if_new(initial);
        }
    }
}

pub(super) fn select_named_presentation_fade_states(
    mut commands: Commands,
    mut work: ParamSet<(
        Query<
            Entity,
            (
                With<NamedPhysicalPresentations>,
                Changed<AuthoredFloatAttributes>,
            ),
        >,
        Query<&NamedPhysicalPresentation, Added<NamedPresentationFade>>,
        (
            Query<(&NamedPhysicalPresentations, &mut AuthoredFloatAttributes)>,
            Query<NamedFadeBinder>,
        ),
    )>,
) {
    let mut affected_owners = work
        .p0()
        .iter()
        .collect::<bevy::ecs::entity::EntityHashSet>();
    affected_owners.extend(work.p1().iter().map(|owner| owner.0));
    let (mut owners, mut binders) = work.p2();
    for owner in affected_owners {
        let Ok((children, mut values)) = owners.get_mut(owner) else {
            continue;
        };
        let mut controllers = BTreeMap::<u32, Vec<_>>::new();
        for (entity, state, ..) in binders.iter_many(&children.0).matched() {
            controllers
                .entry(state.range.controller_ordinal)
                .or_default()
                .push((entity, state.range, state.selected, state.accepted_value));
        }
        for states in controllers.values_mut() {
            states.sort_unstable_by_key(|(_, range, ..)| range.state_ordinal);
            let Some((first, first_range, ..)) = states.first().copied() else {
                continue;
            };
            let Some(value) = values
                .get(first_range.attribute)
                .filter(|value| value.is_finite())
            else {
                continue;
            };
            let active = states.iter().find(|(_, _, selected, _)| *selected);
            if first_range.fade_hold {
                if let Some((_, _, _, Some(previous))) = active {
                    if value != *previous {
                        values.set(first_range.attribute, *previous);
                    }
                } else if let Some((entity, ..)) = active {
                    if let Ok((_, mut state, ..)) = binders.get_mut(*entity) {
                        state.accepted_value = Some(value);
                    }
                }
                continue;
            }
            let selected = active
                .filter(|(_, range, ..)| range.contains(value))
                .or_else(|| states.iter().find(|(_, range, ..)| range.contains(value)));
            let highest = states
                .iter()
                .map(|(_, range, ..)| range.maximum)
                .fold(f32::NEG_INFINITY, f32::max);
            let (target, accepted_value) = if let Some((entity, ..)) = selected {
                (*entity, value)
            } else if first_range.wrap_value && value >= highest {
                let wrapped = first_range.maximum - 1.0;
                values.set(first_range.attribute, wrapped);
                (first, wrapped)
            } else {
                continue;
            };
            if let Ok((_, mut state, ..)) = binders.get_mut(target) {
                if state.accepted_value != Some(accepted_value) {
                    state.accepted_value = Some(accepted_value);
                }
            }
            if active.is_some_and(|(entity, ..)| *entity == target) {
                continue;
            }
            if let Some((entity, ..)) = active {
                change_named_presentation_fade_target(&mut commands, &mut binders, *entity, false);
            }
            change_named_presentation_fade_target(&mut commands, &mut binders, target, true);
        }
    }
}

fn change_named_presentation_fade_target(
    commands: &mut Commands,
    binders: &mut Query<NamedFadeBinder>,
    entity: Entity,
    selected: bool,
) {
    let Ok((_, mut state, mut visibility, mut opacity, transition)) = binders.get_mut(entity)
    else {
        return;
    };
    state.selected = selected;
    let from = if selected { 0.0 } else { state.range.max_alpha };
    let to = if selected { state.range.max_alpha } else { 0.0 };
    let duration = Duration::try_from_secs_f32(state.range.fade_period_seconds)
        .ok()
        .filter(|duration| !duration.is_zero() && from != to);
    if let Some(duration) = duration {
        opacity.set_if_neq(Propagate(NamedPrefabOpacity(from)));
        visibility.set_if_neq(Visibility::Inherited);
        let next = NamedPresentationOpacityTransition {
            timer: Timer::new(duration, TimerMode::Once),
            from,
            to,
        };
        if let Some(mut transition) = transition {
            *transition = next;
        } else {
            commands.entity(entity).try_insert(next);
        }
    } else {
        opacity.set_if_neq(Propagate(NamedPrefabOpacity(to)));
        visibility.set_if_neq(if selected {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        });
        if transition.is_some() {
            commands
                .entity(entity)
                .try_remove::<NamedPresentationOpacityTransition>();
        }
    }
}

pub(super) fn advance_named_presentation_opacity(
    time: Res<Time>,
    mut commands: Commands,
    mut transitions: Query<(
        Entity,
        &NamedPresentationFade,
        &mut NamedPresentationOpacityTransition,
        &mut Propagate<NamedPrefabOpacity>,
        &mut Visibility,
    )>,
) {
    for (entity, state, mut transition, mut opacity, mut visibility) in &mut transitions {
        transition.timer.tick(time.delta());
        let alpha =
            transition.from + (transition.to - transition.from) * transition.timer.fraction();
        opacity.set_if_neq(Propagate(NamedPrefabOpacity(alpha)));
        if transition.timer.is_finished() {
            if !state.selected {
                visibility.set_if_neq(Visibility::Hidden);
            }
            commands
                .entity(entity)
                .try_remove::<NamedPresentationOpacityTransition>();
        }
    }
}

/// A binder holder waiting for the node of its owner's model to exist.
#[derive(Component, Debug, Clone)]
pub(crate) struct NamedPresentationNodeAttachment {
    pub(crate) owner: Entity,
    node: String,
}

/// Moves node-attached binder holders onto their authored node: a prefab
/// entity with that authored attachment name (static model nodes), or an
/// animated skeleton joint with that name. The node may live in a model that
/// loads later, such as a counter attached to the same building, so a holder
/// keeps waiting rather than giving up.
pub(super) fn attach_named_presentations_to_model_nodes(
    mut commands: Commands,
    mut pending: Query<(
        Entity,
        Ref<NamedPresentationNodeAttachment>,
        Option<&mut Visibility>,
    )>,
    indexes: Query<Ref<PrefabSocketIndex>>,
    parents: Query<&ChildOf>,
    changed_parents: Query<(), Changed<ChildOf>>,
) {
    for (holder, attachment, visibility) in &mut pending {
        let Ok(index) = indexes.get(attachment.owner) else {
            continue;
        };
        if !index.is_changed() && !attachment.is_changed() && !changed_parents.contains(holder) {
            continue;
        }
        let key = AssetId::from_key(&attachment.node.trim().to_ascii_lowercase());
        let Some(node_entity) = index.named(key).find(|&candidate| {
            candidate != holder
                && parents
                    .iter_ancestors::<ChildOf>(candidate)
                    .all(|ancestor| ancestor != holder)
        }) else {
            continue;
        };
        if parents
            .get(holder)
            .is_ok_and(|parent| parent.parent() == node_entity)
        {
            continue;
        }
        let mut holder = commands.entity(holder);
        holder.insert(ChildOf(node_entity));
        if let Some(mut visibility) = visibility {
            visibility.set_if_neq(Visibility::Inherited);
        } else {
            holder.insert(Visibility::Inherited);
        }
    }
}

/// A named physical object performs its owner's behaviors, as a food stand's
/// vendor hands over money and food, so its animations belong to the owner.
/// Runs after animation attachment has given the controller its direct parent.
pub(super) fn project_named_presentation_animation_owners(
    mut commands: Commands,
    mut controllers: Query<
        (
            Entity,
            &AnimationPresentationOwner,
            Option<&mut AnimationPresentationBinder>,
        ),
        Changed<AnimationPresentationOwner>,
    >,
    parents: Query<&ChildOf>,
    presentations: Query<(
        &NamedPhysicalPresentation,
        &PrefabAuthoredAttachmentIdentifier,
    )>,
) {
    for (controller, owner, current_binder) in &mut controllers {
        let Some((presentation, identifier)) = parents
            .iter_ancestors::<ChildOf>(controller)
            .find_map(|ancestor| presentations.get(ancestor).ok())
        else {
            if current_binder.is_some() {
                commands
                    .entity(controller)
                    .try_remove::<AnimationPresentationBinder>();
            }
            continue;
        };
        if owner.gameplay_entity != presentation.0 {
            commands
                .entity(controller)
                .try_insert(AnimationPresentationOwner {
                    gameplay_entity: presentation.0,
                });
        }
        let binder = AnimationPresentationBinder(identifier.0);
        if let Some(mut current) = current_binder {
            current.set_if_neq(binder);
        } else {
            commands.entity(controller).try_insert(binder);
        }
    }
}
