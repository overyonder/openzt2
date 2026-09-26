use bevy::{ecs::system::SystemParam, prelude::*};
use openzt2_game_data::ui_document::action::presentation::{
    UiPresentationAction, UiPresentationActionRecord,
};
use openzt2_game_data::ui_document::action::UiTrigger;

use crate::{
    assets::{
        localization::{
            localization_asset_types::LocalizationAsset,
            localization_precedence_index::LocalizationPrecedenceIndex,
        },
        ui_document::ui_document_asset_types_and_borrowing_queries::UiDocumentAsset,
    },
    plugins::audio::audio_playback_message_types::PlayAudioClip,
};

use super::authored_ui_visual_types::UiSourceRect;
use super::{
    animation::UiShowHideAnimation,
    authored_modal_presentation::UiAuthoredModalPresentation,
    authored_timed_action_sequence::UiAuthoredTimedActionSequencePlayback,
    authored_tooltip_presentation::{UiLongTooltipKey, UiShortTooltipKey},
    authored_tree_expansion_and_row_indentation::UiAuthoredTreeExpansion,
    authored_ui_focus_navigation_and_activation::choose_wrapped_authored_ui_focus_candidate,
    authored_ui_focus_state::{UiFocusPresentation, UiFocusScope, UiFocusable},
    authored_ui_integer_range_components::{UiMaximum, UiMinimum},
    authored_ui_interaction_enabled_state_application::SetAuthoredUiNodeInteractionEnabled,
    authored_ui_node_projection_components::UiDocumentOwner,
    authored_ui_node_projection_components::UiDocumentRoot,
    authored_ui_node_projection_components::UiNodeId,
    authored_ui_node_projection_components::UiValue,
    authored_ui_selection_state::UiSelected,
    cursor::UiCursorStyle,
    localized_countdown_presentation::UiLocalizedCountdownPresentation,
    slider::{UiSliderAxis, UiSliderPolicy},
    ui_document_lifecycle_contracts::ShowUiRole,
};
use crate::plugins::ui::authored_ui_action_projection_components::UiPresentationActions;
use crate::plugins::ui::authored_ui_activation_contracts::UiNodeActivated;

#[derive(Message, Debug, Clone)]
pub(crate) struct RequestUiPresentationAction {
    pub(crate) document_root: Entity,
    pub(crate) action: UiPresentationAction,
}

#[derive(Component)]
pub(super) struct PendingUiSelectedChildActivation {
    target_node: openzt2_game_data::AssetId,
    source: crate::plugins::input::input_types::ActionSource,
}

/// Marks a gameplay-owned thought or emote bubble addressed by authored
/// presentation actions.
#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct EmotePresentation;

#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
struct UiAuthoredMessageSequenceCursor(u32);

/// One of the mutually exclusive animated hover pointers in the authored main menu.
#[derive(Component)]
pub(super) struct UiExclusiveHoverPresentation;

#[derive(SystemParam)]
pub(super) struct UiPresentationActionApplicationQueries<'w, 's> {
    scopes: Query<'w, 's, &'static mut UiFocusScope>,
    toggle_groups: Query<
        'w,
        's,
        (),
        With<super::authored_toggle_selection_transitions::UiAuthoredToggleGroupSelectionPolicy>,
    >,
    scroll_positions: Query<'w, 's, &'static mut ScrollPosition>,
    slider_bounds: Query<
        'w,
        's,
        (
            &'static UiSliderPolicy,
            &'static UiMinimum,
            &'static UiMaximum,
        ),
    >,
    layout_nodes: Query<'w, 's, &'static mut Node>,
    texts: Query<
        'w,
        's,
        (
            &'static UiNodeId,
            &'static UiDocumentOwner,
            &'static mut Text,
        ),
    >,
    expanded: Query<'w, 's, &'static mut UiAuthoredTreeExpansion>,
    message_cursors: Query<'w, 's, &'static mut UiAuthoredMessageSequenceCursor>,
    focusables: Query<'w, 's, (Entity, &'static UiFocusable, &'static UiDocumentOwner)>,
    nodes: Query<
        'w,
        's,
        (
            Entity,
            &'static UiNodeId,
            &'static UiDocumentOwner,
            Option<&'static mut Visibility>,
            Option<&'static mut UiValue>,
            Option<&'static mut UiShowHideAnimation>,
            Option<&'static mut UiSelected>,
            Has<UiExclusiveHoverPresentation>,
        ),
    >,
    localized_countdown_presentations: Query<
        'w,
        's,
        (
            &'static UiNodeId,
            &'static UiDocumentOwner,
            &'static mut UiLocalizedCountdownPresentation,
        ),
    >,
    authored_timed_action_sequence_playbacks: Query<
        'w,
        's,
        (
            &'static UiNodeId,
            &'static UiDocumentOwner,
            &'static mut UiAuthoredTimedActionSequencePlayback,
        ),
    >,
    emote_presentations:
        Query<'w, 's, &'static mut Visibility, (With<EmotePresentation>, Without<UiNodeId>)>,
}

/// Applies authored presentation actions to their projected Bevy-tree,
/// document-lifecycle, audio, or global-presentation owners.
pub(in crate::plugins::ui) fn apply_authored_ui_presentation_actions_to_projected_bevy_tree(
    mut commands: Commands,
    mut activations: MessageReader<UiNodeActivated>,
    mut requests: MessageReader<RequestUiPresentationAction>,
    documents: Res<Assets<UiDocumentAsset>>,
    active_localization: Res<LocalizationPrecedenceIndex>,
    localizations: Res<Assets<LocalizationAsset>>,
    sources: Query<&UiDocumentOwner>,
    roots: Query<(Entity, &UiDocumentRoot, &ChildOf)>,
    ranges: Query<&UiPresentationActions>,
    children: Query<&Children>,
    queries: UiPresentationActionApplicationQueries,
    mut enable_changes: MessageWriter<SetAuthoredUiNodeInteractionEnabled>,
    mut show_roles: MessageWriter<ShowUiRole>,
    mut sounds: MessageWriter<PlayAudioClip>,
) {
    let localization = active_localization.borrow_loaded_localization_view(&localizations);
    let UiPresentationActionApplicationQueries {
        mut scopes,
        toggle_groups,
        mut scroll_positions,
        slider_bounds,
        mut layout_nodes,
        mut texts,
        mut expanded,
        mut message_cursors,
        focusables,
        mut nodes,
        mut localized_countdown_presentations,
        mut authored_timed_action_sequence_playbacks,
        mut emote_presentations,
    } = queries;
    let mut pending_actions = Vec::new();
    for activation in activations.read() {
        let (Ok(source_owner), Ok(range)) =
            (sources.get(activation.node), ranges.get(activation.node))
        else {
            continue;
        };
        let Ok((_, root, _)) = roots.get(source_owner.0) else {
            continue;
        };
        let Some(document) = documents.get(&root.document) else {
            continue;
        };
        pending_actions.push((
            activation.clone(),
            *source_owner,
            range
                .authored_action_records(document)
                .cloned()
                .collect::<Vec<_>>(),
        ));
    }
    for request in requests.read() {
        pending_actions.push((
            UiNodeActivated {
                source: crate::plugins::input::input_types::ActionSource::System,
                node: request.document_root,
                trigger: UiTrigger::Press,
            },
            UiDocumentOwner(request.document_root),
            vec![UiPresentationActionRecord {
                trigger: UiTrigger::Press,
                action: request.action.clone(),
            }],
        ));
    }
    for (activation, source_owner, records) in pending_actions {
        let Ok((_, root, lifecycle_owner)) = roots.get(source_owner.0) else {
            continue;
        };
        let Some(document) = documents.get(&root.document) else {
            continue;
        };
        for record in records {
            if activation.trigger != record.trigger {
                continue;
            }
            debug!(
                node = ?activation.node,
                trigger = ?activation.trigger,
                action = ?std::mem::discriminant(&record.action),
                "applying typed UI presentation action"
            );
            match &record.action {
                UiPresentationAction::ShowDocumentRole { document_role } => {
                    show_roles.write(ShowUiRole {
                        role: *document_role,
                        owner: lifecycle_owner.parent(),
                    });
                }
                UiPresentationAction::SetDocumentNodeVisible {
                    document_role,
                    target_node,
                    visible,
                } => {
                    super::cross_document_node_visibility::queue_cross_document_node_visibility(
                        &mut commands,
                        lifecycle_owner.parent(),
                        *document_role,
                        openzt2_game_data::AssetId(target_node.0),
                        *visible,
                    );
                    if *visible {
                        show_roles.write(ShowUiRole {
                            role: *document_role,
                            owner: lifecycle_owner.parent(),
                        });
                    }
                }
                UiPresentationAction::HideDocumentRole { document_role } => {
                    let document_role = *document_role;
                    let lifecycle_owner = lifecycle_owner.parent();
                    for (candidate, root, parent) in &roots {
                        if parent.parent() != lifecycle_owner {
                            continue;
                        }
                        let Some(document) = documents.get(&root.document) else {
                            continue;
                        };
                        if document.canonical_ui_document().role == document_role {
                            if let Some(entry) = children
                                .get(candidate)
                                .ok()
                                .and_then(|children| children.first())
                            {
                                if let Ok((_, _, _, visibility, ..)) = nodes.get_mut(*entry) {
                                    if let Some(mut visibility) = visibility {
                                        *visibility = Visibility::Hidden;
                                    }
                                }
                            }
                        }
                    }
                }
                UiPresentationAction::ActivateTargetNodeWithPress { target_node } => {
                    let target_node = openzt2_game_data::AssetId(target_node.0);
                    if let Some((entity, ..)) = nodes.iter_mut().find(|(_, id, owner, ..)| {
                        owner.0 == source_owner.0 && id.id == target_node
                    }) {
                        let source = activation.source;
                        commands.queue(move |world: &mut World| {
                            world.write_message(UiNodeActivated {
                                source,
                                node: entity,
                                trigger: UiTrigger::Press,
                            });
                        });
                    }
                }
                UiPresentationAction::SetTargetNodeVisible {
                    target_node,
                    visible,
                } => {
                    let target_node = openzt2_game_data::AssetId(target_node.0);
                    let exclusive = nodes.iter_mut().any(|(_, id, owner, .., exclusive)| {
                        owner.0 == source_owner.0 && id.id == target_node && exclusive
                    });
                    if *visible && exclusive {
                        for (_, id, owner, visibility, _, animation, _, candidate_exclusive) in
                            &mut nodes
                        {
                            if owner.0 != source_owner.0
                                || !candidate_exclusive
                                || id.id == target_node
                            {
                                continue;
                            }
                            if let Some(mut animation) = animation {
                                animation.forward = false;
                                animation.elapsed_ms = 0.0;
                                animation.delay_remaining_ms = 0.0;
                                animation.running = false;
                            }
                            if let Some(mut visibility) = visibility {
                                *visibility = Visibility::Hidden;
                            }
                        }
                    }
                    for (_, id, owner, visibility, _, animation, _, _) in &mut nodes {
                        if owner.0 == source_owner.0 && id.id == target_node {
                            let animated = animation.is_some();
                            if let Some(mut animation) = animation {
                                animation.start_authored_visibility_transition(*visible);
                            }
                            if let Some(mut visibility) = visibility {
                                if *visible {
                                    *visibility = Visibility::Inherited;
                                } else if !animated {
                                    *visibility = Visibility::Hidden;
                                }
                            }
                            break;
                        }
                    }
                }
                UiPresentationAction::SetTargetNodeSelectionAndTimedSequencesActive {
                    document_role,
                    target_node,
                    active,
                } => {
                    let target_owner = if let Some(role) = document_role {
                        roots.iter().find_map(|(entity, root, parent)| {
                            (parent.parent() == lifecycle_owner.parent()
                                && documents.get(&root.document).is_some_and(|document| {
                                    document.canonical_ui_document().role == *role
                                }))
                            .then_some(entity)
                        })
                    } else {
                        Some(source_owner.0)
                    };
                    let Some(target_owner) = target_owner else {
                        continue;
                    };
                    let target_node = openzt2_game_data::AssetId(target_node.0);
                    let group_children = (!*active)
                        .then(|| {
                            nodes.iter().find_map(|(entity, id, owner, ..)| {
                                (owner.0 == target_owner
                                    && id.id == target_node
                                    && toggle_groups.contains(entity))
                                .then(|| children.get(entity).ok())
                                .flatten()
                            })
                        })
                        .flatten();
                    for (entity, id, owner, _, _, _, selected, _) in &mut nodes {
                        if owner.0 == target_owner
                            && (id.id == target_node
                                || group_children.is_some_and(|members| members.contains(&entity)))
                        {
                            if let Some(mut selected) = selected {
                                selected.0 = *active;
                            }
                        }
                    }
                    for (id, owner, mut countdown) in &mut localized_countdown_presentations {
                        if owner.0 == target_owner && id.id == target_node {
                            countdown.set_active_and_restart(*active);
                            break;
                        }
                    }
                    for (id, owner, mut sequence) in &mut authored_timed_action_sequence_playbacks {
                        if owner.0 == target_owner && id.id == target_node {
                            sequence.set_active_and_restart(*active);
                            break;
                        }
                    }
                }
                UiPresentationAction::SetTargetNodeInteractionEnabled {
                    target_node,
                    enabled,
                } => {
                    let target_node = openzt2_game_data::AssetId(target_node.0);
                    if let Some((_, id, ..)) = nodes.iter_mut().find(|(_, id, owner, ..)| {
                        owner.0 == source_owner.0 && id.id == target_node
                    }) {
                        enable_changes.write(
                            SetAuthoredUiNodeInteractionEnabled::for_projected_node(
                                source_owner.0,
                                id.index,
                                *enabled,
                            ),
                        );
                    }
                }
                UiPresentationAction::SetTargetNodeIntegerValue {
                    target_node,
                    integer_value,
                } => {
                    let target_node = openzt2_game_data::AssetId(target_node.0);
                    for (entity, id, owner, _, current, _, _, _) in &mut nodes {
                        if owner.0 == source_owner.0 && id.id == target_node {
                            let value = i64::from(*integer_value);
                            if let Some(mut current) = current {
                                current.set_if_neq(UiValue(value));
                            } else {
                                commands.entity(entity).insert(UiValue(value));
                            }
                            break;
                        }
                    }
                }
                UiPresentationAction::ChangeTargetNodeSliderOrScrollPositionByDelta {
                    target_node,
                    position_delta,
                } => {
                    let target_node = openzt2_game_data::AssetId(target_node.0);
                    let target =
                        nodes
                            .iter_mut()
                            .find_map(|(entity, id, owner, _, value, _, _, _)| {
                                (owner.0 == source_owner.0 && id.id == target_node)
                                    .then_some((entity, value))
                            });
                    if let Some((target, value)) = target {
                        if let (Some(mut value), Ok((policy, minimum, maximum))) =
                            (value, slider_bounds.get(target))
                        {
                            let delta = match policy.axis {
                                UiSliderAxis::Horizontal | UiSliderAxis::Both => position_delta[0],
                                UiSliderAxis::Vertical => position_delta[1],
                            };
                            value.set_if_neq(UiValue(crate::plugins::ui::slider::stepped_value(
                                value.0, delta, policy, minimum.0, maximum.0,
                            )));
                        } else if let Ok(mut position) = scroll_positions.get_mut(target) {
                            position.0 +=
                                Vec2::new(position_delta[0] as f32, position_delta[1] as f32);
                        } else {
                            commands.entity(target).insert(ScrollPosition(Vec2::new(
                                position_delta[0] as f32,
                                position_delta[1] as f32,
                            )));
                        }
                    }
                }
                UiPresentationAction::SetTargetNodeImageSourceRectangle {
                    target_node,
                    source_rectangle,
                } => {
                    let target_node = openzt2_game_data::AssetId(target_node.0);
                    if let Some(target) = nodes.iter_mut().find_map(|(entity, id, owner, ..)| {
                        (owner.0 == source_owner.0 && id.id == target_node).then_some(entity)
                    }) {
                        commands
                            .entity(target)
                            .insert(UiSourceRect(source_rectangle.map(|value| value)));
                    }
                }
                UiPresentationAction::SetTargetNodeLayoutPosition {
                    target_node,
                    position,
                } => {
                    let target_node = openzt2_game_data::AssetId(target_node.0);
                    if let Some(target) = nodes.iter_mut().find_map(|(entity, id, owner, ..)| {
                        (owner.0 == source_owner.0 && id.id == target_node).then_some(entity)
                    }) {
                        if let Ok(mut layout) = layout_nodes.get_mut(target) {
                            layout.left = Val::Px(position[0] as f32);
                            layout.top = Val::Px(position[1] as f32);
                        }
                    }
                }
                UiPresentationAction::CopySourceNodeTextToTargetNode {
                    target_node,
                    source_node,
                } => {
                    let source_node = openzt2_game_data::AssetId(source_node.0);
                    let value = texts.iter_mut().find_map(|(id, owner, text)| {
                        (owner.0 == source_owner.0 && id.id == source_node).then(|| text.0.clone())
                    });
                    let target_node = openzt2_game_data::AssetId(target_node.0);
                    if let Some(value) = value {
                        if let Some((_, _, mut text)) = texts.iter_mut().find(|(id, owner, _)| {
                            owner.0 == source_owner.0 && id.id == target_node
                        }) {
                            text.0 = value;
                        }
                    }
                }
                UiPresentationAction::SetTargetNodeTextFromLocalizationKey {
                    target_node,
                    localization_key,
                } => {
                    let target_node = openzt2_game_data::AssetId(target_node.0);
                    let localization_key = openzt2_game_data::AssetId(localization_key.0);
                    if let Some(value) = localization.and_then(|localization| {
                        crate::plugins::ui::localized_ui_text_writing::localized_ui_text(
                            localization,
                            localization_key,
                        )
                    }) {
                        if let Some((_, _, mut text)) = texts.iter_mut().find(|(id, owner, _)| {
                            owner.0 == source_owner.0 && id.id == target_node
                        }) {
                            text.0.clear();
                            text.0.push_str(value);
                        }
                    }
                }
                UiPresentationAction::SetTargetNodeExpanded {
                    target_node,
                    expanded: value,
                } => {
                    let target_node = openzt2_game_data::AssetId(target_node.0);
                    if let Some(target) = nodes.iter_mut().find_map(|(entity, id, owner, ..)| {
                        (owner.0 == source_owner.0 && id.id == target_node).then_some(entity)
                    }) {
                        if let Ok(mut current) = expanded.get_mut(target) {
                            current.set_expanded(*value);
                        } else {
                            commands
                                .entity(target)
                                .insert(UiAuthoredTreeExpansion::from_authored_expansion(*value));
                        }
                    }
                }
                UiPresentationAction::SelectTargetNode { target_node } => {
                    let target_node = openzt2_game_data::AssetId(target_node.0);
                    for (_, id, owner, _, _, _, selected, _) in &mut nodes {
                        if owner.0 == source_owner.0 && id.id == target_node {
                            if let Some(mut selected) = selected {
                                selected.0 = true;
                            }
                            break;
                        }
                    }
                }
                UiPresentationAction::WindTargetShowHideAnimationToCurrentDirectionBoundary {
                    target_node,
                } => {
                    let target_node = openzt2_game_data::AssetId(target_node.0);
                    for (_, id, owner, _, _, animation, _, _) in &mut nodes {
                        if owner.0 == source_owner.0 && id.id == target_node {
                            if let Some(mut animation) = animation {
                                wind_animation(&mut animation);
                            }
                            break;
                        }
                    }
                }
                UiPresentationAction::AdvanceTargetNodeMessageCursor { target_node } => {
                    let target_node = openzt2_game_data::AssetId(target_node.0);
                    if let Some(target) = nodes.iter_mut().find_map(|(entity, id, owner, ..)| {
                        (owner.0 == source_owner.0 && id.id == target_node).then_some(entity)
                    }) {
                        if let Ok(mut cursor) = message_cursors.get_mut(target) {
                            cursor.0 = cursor.0.wrapping_add(1);
                        } else {
                            commands
                                .entity(target)
                                .insert(UiAuthoredMessageSequenceCursor(1));
                        }
                    }
                }
                UiPresentationAction::SetTargetNodeLayoutSize { target_node, size } => {
                    let target_node = openzt2_game_data::AssetId(target_node.0);
                    if let Some(target) = nodes.iter_mut().find_map(|(entity, id, owner, ..)| {
                        (owner.0 == source_owner.0 && id.id == target_node).then_some(entity)
                    }) {
                        if let Ok(mut layout) = layout_nodes.get_mut(target) {
                            layout.width = Val::Px(size[0] as f32);
                            layout.height = Val::Px(size[1] as f32);
                        }
                    }
                }
                // The source action and resolved asset identity remain
                // canonical. No presentation owner currently resolves that
                // identity into the target's Bevy image handle.
                UiPresentationAction::SetTargetNodeImageOverride { .. } => {}
                UiPresentationAction::SetTargetNodeModal { target_node, modal } => {
                    let target_node = openzt2_game_data::AssetId(target_node.0);
                    if let Some(target) = nodes.iter_mut().find_map(|(entity, id, owner, ..)| {
                        (owner.0 == source_owner.0 && id.id == target_node).then_some(entity)
                    }) {
                        commands.entity(target).insert(
                            UiAuthoredModalPresentation::from_authored_modal_state(*modal),
                        );
                    }
                }
                UiPresentationAction::SetTargetNodeTooltip {
                    target_node,
                    localization_key,
                    long_tooltip,
                } => {
                    let target_node = openzt2_game_data::AssetId(target_node.0);
                    if let Some(target) = nodes.iter_mut().find_map(|(entity, id, owner, ..)| {
                        (owner.0 == source_owner.0 && id.id == target_node).then_some(entity)
                    }) {
                        let localization_key = openzt2_game_data::AssetId(localization_key.0);
                        if *long_tooltip {
                            commands
                                .entity(target)
                                .insert(UiLongTooltipKey(localization_key));
                        } else {
                            commands
                                .entity(target)
                                .insert(UiShortTooltipKey(localization_key));
                        }
                    }
                }
                UiPresentationAction::SetTargetNodeCursor {
                    target_node,
                    cursor_asset,
                } => {
                    let target_node = openzt2_game_data::AssetId(target_node.0);
                    if let Some(target) = nodes.iter_mut().find_map(|(entity, id, owner, ..)| {
                        (owner.0 == source_owner.0 && id.id == target_node).then_some(entity)
                    }) {
                        commands
                            .entity(target)
                            .insert(UiCursorStyle(openzt2_game_data::AssetId(cursor_asset.0)));
                    }
                }
                UiPresentationAction::SetTargetNodeTextLiteral {
                    target_node,
                    text: literal_text,
                } => {
                    let target_node = openzt2_game_data::AssetId(target_node.0);
                    if let Some((_, _, mut text)) = texts
                        .iter_mut()
                        .find(|(id, owner, _)| owner.0 == source_owner.0 && id.id == target_node)
                    {
                        text.0.clear();
                        text.0.push_str(literal_text);
                    }
                }
                UiPresentationAction::SetTargetNodeHoverPresentation {
                    target_node,
                    hover_presented,
                } => {
                    let target_node = openzt2_game_data::AssetId(target_node.0);
                    if let Some(target) = nodes.iter_mut().find_map(|(entity, id, owner, ..)| {
                        (owner.0 == source_owner.0 && id.id == target_node).then_some(entity)
                    }) {
                        // Authored programmatic emphasis is presentation state;
                        // physical pointer ownership remains exclusively Bevy's
                        // `Interaction` component.
                        commands
                            .entity(target)
                            .insert(UiFocusPresentation(*hover_presented));
                    }
                }
                UiPresentationAction::ActivateSelectedChildOfTargetNode { target_node } => {
                    let target_node = openzt2_game_data::AssetId(target_node.0);
                    let target = nodes
                        .iter_mut()
                        .find_map(|(entity, id, owner, ..)| {
                            (owner.0 == source_owner.0 && id.id == target_node)
                                .then_some((entity, owner.0))
                        })
                        .or_else(|| {
                            nodes.iter_mut().find_map(|(entity, id, owner, ..)| {
                                (id.id == target_node
                                    && roots.get(owner.0).is_ok_and(|(_, _, parent)| {
                                        parent.parent() == lifecycle_owner.parent()
                                    }))
                                .then_some((entity, owner.0))
                            })
                        });
                    let selected_child = target
                        .and_then(|(target, target_owner)| {
                            children
                                .get(target)
                                .ok()
                                .map(|children| (children, target_owner))
                        })
                        .and_then(|(children, target_owner)| {
                            children.iter().find(|child| {
                                nodes.get_mut(*child).is_ok_and(
                                    |(_, _, owner, _, _, _, selected, _)| {
                                        owner.0 == target_owner
                                            && selected.is_some_and(|selected| selected.0)
                                    },
                                )
                            })
                        });
                    if let Some(selected_child) = selected_child {
                        let source = activation.source;
                        commands.queue(move |world: &mut World| {
                            world.write_message(UiNodeActivated {
                                source,
                                node: selected_child,
                                trigger: UiTrigger::On,
                            });
                        });
                    } else if target.is_none() {
                        commands.spawn((
                            PendingUiSelectedChildActivation {
                                target_node,
                                source: activation.source,
                            },
                            ChildOf(lifecycle_owner.parent()),
                        ));
                    }
                }
                UiPresentationAction::FocusPreviousOrNextSiblingOfTargetNode {
                    target_node,
                    focus_next,
                } => {
                    let target_node = openzt2_game_data::AssetId(target_node.0);
                    let target = nodes.iter_mut().find_map(|(entity, id, owner, ..)| {
                        (owner.0 == source_owner.0 && id.id == target_node).then_some(entity)
                    });
                    let order = target
                        .and_then(|target| focusables.get(target).ok())
                        .map(|(_, focusable, _)| focusable.order);
                    if let Some(order) = order {
                        if let Ok(mut scope) = scopes.get_mut(source_owner.0) {
                            scope.focused = choose_wrapped_authored_ui_focus_candidate(
                                Some(order),
                                if *focus_next { 1 } else { -1 },
                                focusables.iter().filter_map(|(entity, candidate, owner)| {
                                    (owner.0 == source_owner.0 && candidate.enabled)
                                        .then_some((candidate.order, entity))
                                }),
                            );
                        }
                    }
                }
                UiPresentationAction::ShowOnlyNamedChildOfTargetNode {
                    target_node,
                    child_node,
                } => {
                    let target_node = openzt2_game_data::AssetId(target_node.0);
                    let child_node = openzt2_game_data::AssetId(child_node.0);
                    let parent = nodes.iter_mut().find_map(|(entity, id, owner, ..)| {
                        (owner.0 == source_owner.0 && id.id == target_node).then_some(entity)
                    });
                    let Some(parent) = parent else {
                        continue;
                    };
                    let Ok(children) = children.get(parent) else {
                        continue;
                    };
                    for child_entity in children.iter() {
                        let Ok((_, id, owner, visibility, ..)) = nodes.get_mut(child_entity) else {
                            continue;
                        };
                        if owner.0 != source_owner.0 {
                            continue;
                        }
                        if let Some(mut visibility) = visibility {
                            *visibility = if id.id == child_node {
                                Visibility::Inherited
                            } else {
                                Visibility::Hidden
                            };
                        }
                    }
                }
                UiPresentationAction::PlayDocumentAudioCue { audio_cue } => {
                    if let Some(cue) = document.audio_source_handle(*audio_cue) {
                        sounds.write(PlayAudioClip {
                            clip: cue.clone(),
                            emitter: None,
                            selection: 0,
                            force_looped: false,
                        });
                    }
                }
                // The source action remains canonical. No query
                // procedure or result consumer exists yet, so projecting an
                // unread persistent marker would invent completed behavior.
                UiPresentationAction::MarkTargetNodeForQuery { .. } => {}
                UiPresentationAction::HideOwningDocument
                | UiPresentationAction::HideOwningDocumentAfterAlertAcknowledgement
                | UiPresentationAction::HideOwningDocumentAfterConfirmationDismissal => {
                    if let Some((_, _, _, Some(mut visibility), ..)) =
                        nodes.iter_mut().find(|(entity, _, owner, ..)| {
                            *entity == source_owner.0 && owner.0 == source_owner.0
                        })
                    {
                        *visibility = Visibility::Hidden;
                    }
                }
                UiPresentationAction::SetAllEmotePresentationsVisible { visible } => {
                    let visibility = if *visible {
                        Visibility::Inherited
                    } else {
                        Visibility::Hidden
                    };
                    for mut current in &mut emote_presentations {
                        *current = visibility;
                    }
                }
            }
        }
    }
}

pub(super) fn activate_pending_selected_children_after_ui_document_projection(
    mut commands: Commands,
    pending: Query<(Entity, &PendingUiSelectedChildActivation, &ChildOf)>,
    roots: Query<&ChildOf, With<UiDocumentRoot>>,
    nodes: Query<(Entity, &UiNodeId, &UiDocumentOwner, Option<&UiSelected>)>,
    children: Query<&Children>,
    mut activations: MessageWriter<UiNodeActivated>,
) {
    for (pending_entity, pending_activation, lifecycle_owner) in &pending {
        let target = nodes.iter().find_map(|(entity, id, owner, _)| {
            (id.id == pending_activation.target_node
                && roots
                    .get(owner.0)
                    .is_ok_and(|parent| parent.parent() == lifecycle_owner.parent()))
            .then_some((entity, owner.0))
        });
        let Some((target, target_owner)) = target else {
            continue;
        };
        if let Some(selected_child) = children.get(target).ok().and_then(|children| {
            children.iter().find(|child| {
                nodes.get(*child).is_ok_and(|(_, _, owner, selected)| {
                    owner.0 == target_owner && selected.is_some_and(|selected| selected.0)
                })
            })
        }) {
            activations.write(UiNodeActivated {
                source: pending_activation.source,
                node: selected_child,
                trigger: UiTrigger::On,
            });
        }
        commands.entity(pending_entity).despawn();
    }
}

fn wind_animation(animation: &mut crate::plugins::ui::animation::UiShowHideAnimation) {
    animation.elapsed_ms = if animation.forward {
        animation.duration_ms
    } else {
        0.0
    };
    animation.running = true;
}
