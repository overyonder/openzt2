use super::prefab_attachment_independent_rotation::PrefabAttachmentIndependentRotation;
use openzt2_world_state::world_spawn::prefab_authored_attachment_identifier::PrefabAuthoredAttachmentIdentifier;
use openzt2_world_state::world_spawn::prefab_presentation_types::PrefabPresentation;
use openzt2_world_state::world_spawn::prefab_presentation_types::PrefabCausticPolicy;
use openzt2_world_state::world_spawn::world_membership_types::DefinitionId;
use openzt2_world_state::animation_graph::animation_presentation_relationship_types::AnimationPresentationOwner;
use bevy::prelude::*;
use openzt2_game_assets::scene_prefab::ScenePrefabAsset;
use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use openzt2_game_assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use openzt2_game_data::world_definitions::world_objects::WorldObjectNamedPhysicalPresentationDefinition;
use openzt2_game_data::AssetId;

#[cfg(test)]
mod node_attachment_tests;
#[cfg(test)]
mod tests;

#[derive(Component)]
pub(super) struct NamedPhysicalPresentationsHydrated;

/// A named physical object retaining its owner after attachment to a model node.
#[derive(Component)]
pub(super) struct NamedPhysicalPresentation(Entity);

/// Borrows each physical binder's policy without applying it to sibling binders.
pub(super) fn project_physical_presentation_caustic_policies(
    mut commands: Commands,
    active_definitions: Res<WorldDefinitions>,
    assets: Res<Assets<WorldDefinitionAsset>>,
    presentations: Query<
        (
            Entity,
            Option<&DefinitionId>,
            Option<&PrefabAuthoredAttachmentIdentifier>,
            Option<&PrefabCausticPolicy>,
        ),
        Or<(With<DefinitionId>, With<PrefabPresentation>)>,
    >,
    parents: Query<&ChildOf>,
    definitions: Query<&DefinitionId>,
) {
    let catalogue = active_definitions.get(&assets);
    for (entity, own_definition, binder, current) in &presentations {
        let definition = own_definition.or_else(|| {
            parents
                .iter_ancestors::<ChildOf>(entity)
                .find_map(|ancestor| definitions.get(ancestor).ok())
        });
        let Some(object) = definition.and_then(|id| catalogue.as_ref()?.find_object(id.0)) else {
            if current.is_some() {
                commands.entity(entity).remove::<PrefabCausticPolicy>();
            }
            continue;
        };
        let binder = if own_definition.is_some() {
            AssetId::from_key("mainobj")
        } else if let Some(binder) = binder {
            binder.0
        } else {
            if current.is_some() {
                commands.entity(entity).remove::<PrefabCausticPolicy>();
            }
            continue;
        };
        let Some(policy) = object
            .presentation_caustic_policies
            .iter()
            .find(|policy| policy.binder == binder)
        else {
            if current.is_some() {
                commands.entity(entity).remove::<PrefabCausticPolicy>();
            }
            continue;
        };
        let projected = PrefabCausticPolicy {
            receives: !policy.no_caustics && !policy.is_decal,
        };
        if current != Some(&projected) {
            commands.entity(entity).insert(projected);
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
    }
    commands
        .entity(owner)
        .insert(NamedPhysicalPresentationsHydrated);
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
    mut controllers: Query<
        (Entity, &mut AnimationPresentationOwner),
        Changed<AnimationPresentationOwner>,
    >,
    parents: Query<&ChildOf>,
    presentations: Query<&NamedPhysicalPresentation>,
) {
    for (controller, mut owner) in &mut controllers {
        let Some(presentation_owner) = parents
            .iter_ancestors::<ChildOf>(controller)
            .find_map(|ancestor| presentations.get(ancestor).ok())
            .map(|presentation| presentation.0)
        else {
            continue;
        };
        if owner.gameplay_entity != presentation_owner {
            owner.gameplay_entity = presentation_owner;
        }
    }
}
