use bevy::prelude::*;

use openzt2_game_assets::animation::animation_set_asset_types::AnimationSetAsset;

use crate::plugins::{
    animation_graph::animation_graph_playback_message_types::AnimationClipPlaybackRequest,
    animation_graph::animation_presentation_relationship_types::AnimationPresentationOwner,
    animation_graph::model_animation_asset_binding_types::{
        ModelAnimationAttachmentResolved, PendingModelAnimationAssets,
    },
    animation_playback::animation_playback_controller_types::AnimationPlaybackController,
    animation_playback::animation_playback_controller_types::AnimationPlaybackRepetitionPolicy,
};

use super::{
    gate_actor_type_queries::GateActorTypeQueries,
    gate_operation_types::{Gate, GateAutoClose, SetGateState},
    topology_edit_types::TopologyChanged,
    topology_graph_types::{FenceEdge, TopologyNode},
    topology_grid_geometry::calculate_topology_edge_cell_bounds,
};

pub(super) fn apply_requested_gate_state(
    mut requests: MessageReader<SetGateState>,
    definitions: GateActorTypeQueries,
    mut gates: Query<(&mut Gate, &FenceEdge, Option<&mut GateAutoClose>)>,
    nodes: Query<&TopologyNode>,
    mut changed: MessageWriter<TopologyChanged>,
) {
    let Some(definitions) = definitions.definitions() else {
        return;
    };
    for request in requests.read() {
        let Ok((mut gate, edge, automatic_close)) = gates.get_mut(request.gate) else {
            continue;
        };
        if gate.locked || gate.open == request.open {
            continue;
        }
        let (Ok(first_node), Ok(second_node)) = (nodes.get(edge.a), nodes.get(edge.b)) else {
            continue;
        };
        gate.open = request.open;
        if let Some(definition) = definitions.find_fence(edge.definition) {
            let policy = &definition.gate_policy;
            if let Some(mut automatic_close) = automatic_close {
                automatic_close.0 = request.open.then_some(policy.auto_close_ticks).unwrap_or(0);
            }
        }
        changed.write(TopologyChanged {
            transaction: None,
            bounds: calculate_topology_edge_cell_bounds(first_node.cell, second_node.cell),
        });
    }
}

/// Native gate models do not embed their actor manifest's clips. Once a gate
/// model resolves without animation, attach the manifest named by its fence
/// definition, starting from the clip for the gate's current state.
pub(super) fn attach_authored_animation_sets_to_resolved_gate_models(
    mut commands: Commands,
    definitions: GateActorTypeQueries,
    animation_set_assets: Res<Assets<AnimationSetAsset>>,
    resolved_models: Query<
        Entity,
        (
            Added<ModelAnimationAttachmentResolved>,
            Without<AnimationPlaybackController>,
        ),
    >,
    parents: Query<&ChildOf>,
    presentation_owners: Query<&AnimationPresentationOwner>,
    gates: Query<(&Gate, &FenceEdge)>,
) {
    let Some(definitions) = definitions.definitions() else {
        return;
    };
    for model in &resolved_models {
        let Some((gate, edge)) = parents
            .iter_ancestors(model)
            .find_map(|ancestor| presentation_owners.get(ancestor).ok())
            .and_then(|owner| gates.get(owner.gameplay_entity).ok())
        else {
            continue;
        };
        let Some(definition) = definitions.find_fence(edge.definition) else {
            continue;
        };
        let clip = if gate.open {
            &definition.gate_policy.open_animation
        } else {
            &definition.gate_policy.close_animation
        };
        let Some(animation_set) =
            definitions.load_fence_gate_animation_set::<AnimationSetAsset>(edge.definition)
        else {
            continue;
        };
        if clip.is_empty()
            || animation_set_assets
                .get(&animation_set)
                .is_some_and(|set| set.animation_clip_asset_load_has_failed(clip))
        {
            continue;
        }
        commands
            .entity(model)
            .insert(PendingModelAnimationAssets {
                pending_model_animation_set_asset_handles: vec![animation_set].into_boxed_slice(),
                requested_initial_animation_clip_asset_key: Some(clip.clone()),
            })
            .remove::<ModelAnimationAttachmentResolved>();
    }
}

/// Initial and restored state are projected when the model becomes ready;
/// subsequent state changes address that same gate-animation presentation.
pub(super) fn project_gate_state_to_ready_animation_presentations(
    definitions: GateActorTypeQueries,
    gates: Query<(Ref<Gate>, &FenceEdge)>,
    controllers: Query<(
        Entity,
        Ref<AnimationPlaybackController>,
        &AnimationPresentationOwner,
    )>,
    mut animations: MessageWriter<AnimationClipPlaybackRequest>,
) {
    let Some(definitions) = definitions.definitions() else {
        return;
    };
    for (controller_entity, controller, owner) in &controllers {
        let Ok((gate, edge)) = gates.get(owner.gameplay_entity) else {
            continue;
        };
        if !gate.is_changed() && !controller.is_added() {
            continue;
        }
        let Some(definition) = definitions.find_fence(edge.definition) else {
            continue;
        };
        let clip = if gate.open {
            &definition.gate_policy.open_animation
        } else {
            &definition.gate_policy.close_animation
        };
        if !clip.is_empty() {
            animations.write(AnimationClipPlaybackRequest {
                animation_subject_entity: controller_entity,
                animation_clip_asset_key: clip.clone(),
                blend_duration_milliseconds: 0,
                playback_speed_permille: 1000,
                playback_repetition_policy: AnimationPlaybackRepetitionPolicy::PlayOnce,
            });
        }
    }
}
