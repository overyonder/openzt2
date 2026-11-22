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
use openzt2_zoo_state::feeding::container_quantity::{DrinkContainer, FoodContainer};
use openzt2_zoo_state::aquatic::aquatic_simulation_types::WaterFilter;
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
                PendingNamedPresentationNodeAttachment {
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
            NamedPhysicalPresentation(owner),
        ));
        if let Some(prefab) = prefab {
            child.insert(PrefabPresentation::new(prefab));
        }
        if let Some(fade) = presentation.fade {
            child.insert(NamedPresentationFade(fade));
        }
    }
    commands
        .entity(owner)
        .insert(NamedPhysicalPresentationsHydrated);
}

/// A `BFAIFadeController` state binder, shown while its owner's attribute
/// lies in its range.
#[derive(Component)]
pub(super) struct NamedPresentationFade(WorldObjectFadeRange);

/// Shows each fade state binder whose range holds its owner's attribute and
/// hides the others. Binders whose attribute has no live value here keep their
/// authored visibility.
pub(super) fn select_named_presentation_fade_states(
    owners: Query<(
        &NamedPhysicalPresentations,
        Option<&FoodContainer>,
        Option<&DrinkContainer>,
        Option<&WaterFilter>,
    )>,
    changed_values: Query<
        Entity,
        (
            With<NamedPhysicalPresentations>,
            Or<(
                Changed<FoodContainer>,
                Changed<DrinkContainer>,
                Changed<WaterFilter>,
            )>,
        ),
    >,
    mut removed_food: RemovedComponents<FoodContainer>,
    mut states: ParamSet<(
        Query<
            &NamedPhysicalPresentation,
            (
                With<NamedPresentationFade>,
                Or<(
                    Changed<NamedPhysicalPresentation>,
                    Changed<NamedPresentationFade>,
                    Changed<Visibility>,
                )>,
            ),
        >,
        Query<(&NamedPresentationFade, &mut Visibility)>,
    )>,
) {
    let food_level = AssetId::from_key("f_foodlevel");
    let filter_dirty_level = AssetId::from_key("f_filterdirtylevel");
    let changed_owners = changed_values
        .iter()
        .chain(states.p0().iter().map(|owner| owner.0))
        .chain(removed_food.read())
        .collect::<HashSet<_>>();
    let mut states = states.p1();
    for (presentations, food, drink, filter) in owners.iter_many(changed_owners).matched() {
        // Container amounts are authored points in Q16.
        let food_amount = food
            .map(|food| food.amount_q16)
            .or(drink.map(|drink| drink.amount_q16))
            .map(|amount| amount as f32 / 65_536.0);
        let mut binders = states.iter_many_mut(&presentations.0).matched();
        while let Some((fade, mut visibility)) = binders.fetch_next() {
            let value = if fade.0.attribute == food_level {
                food_amount
            } else if fade.0.attribute == filter_dirty_level {
                filter.map(|filter| filter.dirt_percent)
            } else {
                None
            };
            if let Some(value) = value {
                visibility.set_if_neq(if fade.0.contains(value) {
                    Visibility::Inherited
                } else {
                    Visibility::Hidden
                });
            }
        }
    }
}

/// A binder holder waiting for the node of its owner's model to exist.
#[derive(Component, Debug, Clone)]
pub(super) struct PendingNamedPresentationNodeAttachment {
    owner: Entity,
    node: String,
}

/// Moves node-attached binder holders onto their authored node: a prefab
/// entity with that authored attachment name (static model nodes), or an
/// animated skeleton joint with that name. The node may live in a model that
/// loads later, such as a counter attached to the same building, so a holder
/// keeps waiting rather than giving up.
pub(super) fn attach_named_presentations_to_model_nodes(
    mut commands: Commands,
    pending: Query<(Entity, &PendingNamedPresentationNodeAttachment)>,
    children: Query<&Children>,
    parents: Query<&ChildOf>,
    identifiers: Query<&PrefabAuthoredAttachmentIdentifier>,
    names: Query<&Name>,
) {
    for (holder, attachment) in &pending {
        let node_name = attachment.node.trim();
        let node = AssetId::from_key(&node_name.to_ascii_lowercase());
        let Some(node_entity) = children
            .iter_descendants_depth_first::<Children>(attachment.owner)
            .filter(|candidate| {
                *candidate != holder
                    && !parents
                        .iter_ancestors::<ChildOf>(*candidate)
                        .any(|ancestor| ancestor == holder)
            })
            .find(|candidate| {
                identifiers.get(*candidate).is_ok_and(|id| id.0 == node)
                    || names
                        .get(*candidate)
                        .is_ok_and(|name| name.as_str().eq_ignore_ascii_case(node_name))
            })
        else {
            continue;
        };
        commands
            .entity(holder)
            .insert((ChildOf(node_entity), Visibility::Inherited))
            .remove::<PendingNamedPresentationNodeAttachment>();
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
