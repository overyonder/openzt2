//! Guest tasks address the gameplay entity, not an intermediate prefab node.

use bevy::prelude::*;
use openzt2_game_data::AssetId;
use openzt2_game_assets::animation::animation_set_asset_types::AnimationSetAsset;
use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use openzt2_game_assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;

use openzt2_world_state::animation_graph::animation_graph_playback_state_types::AnimationGraphPlaybackState;
use openzt2_world_state::animation_graph::animation_presentation_relationship_types::AnimationPresentationOwner;
use openzt2_world_state::animation_graph::model_animation_asset_attachment::select_initial_animation_clip_asset_key_from_graph_node;
use openzt2_world_state::animation_graph::model_animation_asset_binding_types::ModelAnimationAttachmentResolved;
use openzt2_world_state::animation_graph::model_animation_asset_binding_types::PendingModelAnimationAssets;
use openzt2_world_state::animation_playback::animation_playback_controller_types::AnimationPlaybackController;
use openzt2_world_state::world_spawn::world_membership_types::DefinitionId;

use super::guest_simulation_types::{Guest, GuestArchetype};

/// A guest body model waiting for its actor manifest to load, so its first
/// clip can be chosen from the authored stationary graph node.
#[derive(Component)]
pub(super) struct GuestBodyAnimationSetPending {
    guest_definition: AssetId,
    animation_set: Handle<AnimationSetAsset>,
}

/// Native guest body scenes do not carry their actor manifest's clips. Once a
/// guest's own body model resolves without animation, request the manifest its
/// definition names. Models of held items stop at their own definition and are
/// left alone.
pub(super) fn request_authored_animation_sets_for_resolved_guest_models(
    mut commands: Commands,
    definition_assets: Res<Assets<WorldDefinitionAsset>>,
    world_definitions: Res<WorldDefinitions>,
    resolved_models: Query<
        (
            Entity,
            Has<AnimationPlaybackController>,
            Has<AnimationGraphPlaybackState>,
        ),
        Added<ModelAnimationAttachmentResolved>,
    >,
    parents: Query<&ChildOf>,
    owners: Query<(Option<&GuestArchetype>, Has<DefinitionId>)>,
) {
    let Some(definitions) = world_definitions.get(&definition_assets) else {
        return;
    };
    for (model, has_controller, has_graph) in &resolved_models {
        let owner = parents.iter_ancestors(model).find_map(|ancestor| {
            let (archetype, defined) = owners.get(ancestor).ok()?;
            (archetype.is_some() || defined).then_some((ancestor, archetype))
        });
        let Some((guest, Some(archetype))) = owner else {
            continue;
        };
        debug!(?model, ?guest, guest_definition = ?archetype.0, has_controller, has_graph,
            "guest-owned model resolved its animation attachment");
        if has_controller {
            continue;
        }
        let Some(animation_set) =
            definitions.load_guest_model_animation_set::<AnimationSetAsset>(archetype.0)
        else {
            debug!(?model, guest = ?archetype.0, "guest body model has no authored actor manifest");
            continue;
        };
        commands.entity(model).insert(GuestBodyAnimationSetPending {
            guest_definition: archetype.0,
            animation_set,
        });
    }
}

/// Attaches a loaded guest manifest, starting in the preferred clip of the
/// guest's authored stationary node (`Stand_Idle` for `Stand`).
pub(super) fn attach_loaded_animation_sets_to_guest_models(
    mut commands: Commands,
    definition_assets: Res<Assets<WorldDefinitionAsset>>,
    world_definitions: Res<WorldDefinitions>,
    animation_sets: Res<Assets<AnimationSetAsset>>,
    pending: Query<(Entity, &GuestBodyAnimationSetPending)>,
) {
    let Some(definitions) = world_definitions.get(&definition_assets) else {
        return;
    };
    for (model, request) in &pending {
        let Some(animation_set) = animation_sets.get(&request.animation_set) else {
            continue;
        };
        let initial_clip = definitions
            .find_guest(request.guest_definition)
            .and_then(|guest| guest.stationary_animation_graph_node.as_deref())
            .and_then(|node| {
                animation_set
                    .animation_graph_nodes
                    .iter()
                    .find(|graph_node| graph_node.animation_graph_node_asset_key == node)
            })
            .and_then(|graph_node| {
                select_initial_animation_clip_asset_key_from_graph_node(animation_set, graph_node)
            });
        debug!(
            ?model,
            graph_nodes = animation_set.animation_graph_nodes.len(),
            ?initial_clip,
            "attaching guest actor manifest"
        );
        commands
            .entity(model)
            .insert(PendingModelAnimationAssets {
                pending_model_animation_set_asset_handles: vec![request.animation_set.clone()]
                    .into_boxed_slice(),
                requested_initial_animation_clip_asset_key: initial_clip,
            })
            .remove::<(
                ModelAnimationAttachmentResolved,
                GuestBodyAnimationSetPending,
            )>();
    }
}

/// Runs in Presentation, after animation attachment in Intent has inserted its
/// controller and initial direct-parent owner. Reattachment also changes that
/// component, so a hot-reloaded model follows the same hierarchy resolution.
pub(super) fn project_guest_animation_owners(
    mut controllers: Query<
        (Entity, &mut AnimationPresentationOwner),
        Changed<AnimationPresentationOwner>,
    >,
    parents: Query<&ChildOf>,
    guests: Query<(), With<Guest>>,
) {
    for (controller, mut owner) in &mut controllers {
        if guests.contains(owner.gameplay_entity) {
            continue;
        }
        if let Some(guest) = parents
            .iter_ancestors::<ChildOf>(controller)
            .find(|ancestor| guests.contains(*ancestor))
        {
            owner.gameplay_entity = guest;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nested_guest_controller_and_reattachment_keep_gameplay_owner() {
        let mut app = App::new();
        app.add_systems(Update, project_guest_animation_owners);
        let guest = app.world_mut().spawn(Guest).id();
        let prefab_root = app.world_mut().spawn(ChildOf(guest)).id();
        let controller = app
            .world_mut()
            .spawn((
                ChildOf(prefab_root),
                AnimationPresentationOwner {
                    gameplay_entity: prefab_root,
                },
            ))
            .id();
        app.update();
        assert_eq!(
            app.world()
                .get::<AnimationPresentationOwner>(controller)
                .unwrap()
                .gameplay_entity,
            guest
        );
        app.world_mut()
            .entity_mut(controller)
            .insert(AnimationPresentationOwner {
                gameplay_entity: prefab_root,
            });
        app.update();
        assert_eq!(
            app.world()
                .get::<AnimationPresentationOwner>(controller)
                .unwrap()
                .gameplay_entity,
            guest
        );
    }
}
