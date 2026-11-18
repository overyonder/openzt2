use bevy::{ecs::system::SystemParam, prelude::*};
use openzt2_game_data::ui_document::action::presentation::UiPresentationAction;
use openzt2_game_data::ui_document::action::UiTrigger;

use openzt2_game_assets::localization::localization_asset_types::LocalizationAsset;
use openzt2_game_assets::localization::localization_precedence_index::LocalizationPrecedenceIndex;
use openzt2_game_assets::ui_document::ui_document_asset_types_and_borrowing_queries::UiDocumentAsset;
use openzt2_zoo_state::audio::audio_playback_message_types::PlayAudioClip;

use super::authored_ui_visual_types::{UiSourceRect, UiVisualLayer};
use super::{
    animation::UiShowHideAnimation,
    authored_modal_presentation::UiAuthoredModalPresentation,
    authored_reusable_list_and_table_runtime_types::UiTypeListPolicy,
    authored_timed_action_sequence::UiAuthoredTimedActionSequencePlayback,
    authored_tooltip_presentation::{UiLongTooltipKey, UiShortTooltipKey},
    authored_tree_expansion_and_row_indentation::UiAuthoredTreeExpansion,
    authored_ui_change_activation_dispatch::{
        apply_explicit_authored_selection, UiPreviousSelection,
    },
    authored_ui_focus_navigation_and_activation::choose_wrapped_authored_ui_focus_candidate,
    authored_ui_focus_state::{UiFocusPresentation, UiFocusScope, UiFocusable},
    authored_ui_integer_range_components::{UiMaximum, UiMinimum},
    authored_ui_interaction_enabled_state_application::SetAuthoredUiNodeInteractionEnabled,
    authored_ui_node_projection_components::UiDocumentOwner,
    authored_ui_node_projection_components::UiDocumentRoot,
    authored_ui_node_projection_components::UiNodeId,
    authored_ui_node_projection_components::UiValue,
    authored_ui_selection_state::UiSelected,
    authored_ui_target_lookup::AuthoredUiTargetLookup,
    cursor::UiCursorStyle,
    slider::{UiSliderAxis, UiSliderPolicy},
};
use openzt2_interface_state::ui::authored_ui_action_projection_components::UiPresentationActions;
use openzt2_interface_state::ui::authored_ui_activation_contracts::UiNodeActivated;
use openzt2_interface_state::ui::ui_document_lifecycle_contracts::{
    AuthoredUiDocumentOperation, UiDocumentOperation,
};

#[derive(Message, Debug, Clone)]
pub struct RequestUiPresentationAction {
    pub document_root: Entity,
    pub action: UiPresentationAction,
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
    pointer_capture: Res<'w, openzt2_interface_state::ui::pointer_capture_types::UiPointerCapture>,
    scopes: Query<'w, 's, &'static mut UiFocusScope>,
    node_parents: Query<'w, 's, &'static ChildOf>,
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
    target_lookup: AuthoredUiTargetLookup<'w, 's>,
    modals: Query<'w, 's, &'static mut UiAuthoredModalPresentation>,
    long_tooltips: Query<'w, 's, &'static mut UiLongTooltipKey>,
    short_tooltips: Query<'w, 's, &'static mut UiShortTooltipKey>,
    cursors: Query<'w, 's, &'static mut UiCursorStyle>,
    image_nodes: Query<'w, 's, (&'static mut ImageNode, Has<UiVisualLayer>)>,
    source_rectangles: Query<'w, 's, &'static mut UiSourceRect>,
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
    authored_timed_action_sequence_playbacks: Query<
        'w,
        's,
        (
            Entity,
            &'static UiNodeId,
            &'static UiDocumentOwner,
            &'static mut UiAuthoredTimedActionSequencePlayback,
        ),
    >,
    emote_presentations:
        Query<'w, 's, &'static mut Visibility, (With<EmotePresentation>, Without<UiNodeId>)>,
    visibilities: Query<'w, 's, &'static InheritedVisibility>,
    previous_selections: Query<'w, 's, (&'static mut UiPreviousSelection, Has<UiTypeListPolicy>)>,
}

/// Applies authored presentation actions to their projected Bevy-tree,
/// document-lifecycle, audio, or global-presentation owners.
pub(crate) fn apply_authored_ui_presentation_actions_to_projected_bevy_tree(
    mut commands: Commands,
    mut activations: ParamSet<(
        MessageReader<UiNodeActivated>,
        MessageWriter<UiNodeActivated>,
    )>,
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
    mut document_operations: MessageWriter<AuthoredUiDocumentOperation>,
    control: Option<
        Res<openzt2_world_state::simulation_time::simulation_control_types::SimulationControl>,
    >,
    mut sounds: MessageWriter<PlayAudioClip>,
) {
    let localization = active_localization.borrow_loaded_localization_view(&localizations);
    let UiPresentationActionApplicationQueries {
        pointer_capture,
        mut scopes,
        node_parents,
        toggle_groups,
        mut scroll_positions,
        slider_bounds,
        mut layout_nodes,
        target_lookup,
        mut modals,
        mut long_tooltips,
        mut short_tooltips,
        mut cursors,
        mut image_nodes,
        mut source_rectangles,
        mut texts,
        mut expanded,
        mut message_cursors,
        focusables,
        mut nodes,
        mut authored_timed_action_sequence_playbacks,
        mut emote_presentations,
        visibilities,
        mut previous_selections,
    } = queries;
    let mut forwarded = Vec::new();
    let mut incoming = activations.p0();
    let actions = incoming
        .read()
        .filter_map(|activation| {
            let source_owner = sources.get(activation.node).ok()?;
            let range = ranges.get(activation.node).ok()?;
            let (_, root, _) = roots.get(source_owner.0).ok()?;
            let document = documents.get(&root.document)?;
            Some((activation, *source_owner, range, document))
        })
        .flat_map(|(activation, owner, range, document)| {
            range
                .authored_action_records(document)
                .filter(move |record| record.trigger == activation.trigger)
                .map(move |record| ((*activation).clone(), owner, &record.action))
        })
        .chain(requests.read().map(|request| {
            (
                UiNodeActivated {
                    source: openzt2_interface_state::input::input_types::ActionSource::System,
                    node: request.document_root,
                    trigger: UiTrigger::Press,
                },
                UiDocumentOwner(request.document_root),
                &request.action,
            )
        }));
    for (activation, source_owner, action) in actions {
        let Ok((_, root, lifecycle_owner)) = roots.get(source_owner.0) else {
            continue;
        };
        let Some(document) = documents.get(&root.document) else {
            continue;
        };
        match action {
            UiPresentationAction::ShowDocumentRole { document_role } => {
                document_operations.write(AuthoredUiDocumentOperation {
                    owner: lifecycle_owner.parent(),
                    role: *document_role,
                    operation: UiDocumentOperation::Open,
                    transfer_pause: control
                        .as_deref()
                        .is_some_and(|control| control.temporary_ui_pause_is_held()),
                });
            }
            UiPresentationAction::SetDocumentNodeVisible {
                document_role,
                target_node,
                visible,
            } => {
                document_operations.write(AuthoredUiDocumentOperation {
                    owner: lifecycle_owner.parent(),
                    role: *document_role,
                    operation: UiDocumentOperation::ShowNode {
                        target: *target_node,
                        visible: *visible,
                    },
                    transfer_pause: control
                        .as_deref()
                        .is_some_and(|control| control.temporary_ui_pause_is_held()),
                });
            }
            UiPresentationAction::HideDocumentRole { document_role } => {
                document_operations.write(AuthoredUiDocumentOperation {
                    owner: lifecycle_owner.parent(),
                    role: *document_role,
                    operation: UiDocumentOperation::Hide,
                    transfer_pause: false,
                });
            }
            UiPresentationAction::ActivateTargetNodeWithPress { target_node } => {
                let target_node = openzt2_game_data::AssetId(target_node.0);
                // Authored names repeat across sibling dialogs, such as each
                // confirmation's "cancel". Press the one the player can see.
                let targets = || target_lookup.candidates(source_owner.0, target_node);
                let first_visible = targets()
                    .find(|entity| visibilities.get(*entity).is_ok_and(|visible| visible.get()));
                if let Some(entity) = first_visible.or_else(|| targets().next()) {
                    let source = activation.source;
                    forwarded.push(UiNodeActivated {
                        source,
                        node: entity,
                        trigger: UiTrigger::Press,
                    });
                }
            }
            UiPresentationAction::HideTargetNodeUnlessHovered { target_node } => {
                for (entity, id, owner, visibility, _, animation, ..) in &mut nodes {
                    if owner.0 != source_owner.0 || id.id != *target_node {
                        continue;
                    }
                    let hovered = pointer_capture.target.is_some_and(|hovered| {
                        hovered == entity
                            || children
                                .iter_descendants(entity)
                                .any(|child| child == hovered)
                    });
                    if hovered {
                        break;
                    }
                    if let Some(mut animation) = animation {
                        animation.start_authored_visibility_transition(false);
                    } else if let Some(mut visibility) = visibility {
                        *visibility = Visibility::Hidden;
                    }
                    break;
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
                        if owner.0 != source_owner.0 || !candidate_exclusive || id.id == target_node
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
                // Turning on a member of a toggle set turns its selected sibling
                // off first: tabs share containers, and the loser hides them.
                if *active {
                    let target = target_lookup.candidates(target_owner, target_node).next();
                    let siblings = target
                        .and_then(|target| node_parents.get(target).ok())
                        .map(ChildOf::parent)
                        .filter(|group| toggle_groups.contains(*group))
                        .and_then(|group| children.get(group).ok());
                    for sibling in siblings.into_iter().flatten() {
                        if Some(*sibling) == target {
                            continue;
                        }
                        let Ok((_, _, _, _, _, _, Some(mut selected), _)) = nodes.get_mut(*sibling)
                        else {
                            continue;
                        };
                        let Ok((mut previous, is_type_list)) =
                            previous_selections.get_mut(*sibling)
                        else {
                            selected.0 = false;
                            continue;
                        };
                        if let Some(trigger) = apply_explicit_authored_selection(
                            &mut selected,
                            &mut previous,
                            false,
                            is_type_list,
                        ) {
                            let source = activation.source;
                            let node = *sibling;
                            forwarded.push(UiNodeActivated {
                                source,
                                node,
                                trigger,
                            });
                        }
                    }
                }
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
                            let Ok((mut previous, is_type_list)) =
                                previous_selections.get_mut(entity)
                            else {
                                selected.0 = *active;
                                continue;
                            };
                            // Explicit off/on requests can occur in one frame. Dispatch
                            // each transition here before the change observer coalesces them.
                            let Some(trigger) = apply_explicit_authored_selection(
                                &mut selected,
                                &mut previous,
                                *active,
                                is_type_list,
                            ) else {
                                continue;
                            };
                            let source = activation.source;
                            forwarded.push(UiNodeActivated {
                                source,
                                node: entity,
                                trigger,
                            });
                        }
                    }
                }
                for (entity, id, owner, mut sequence) in
                    &mut authored_timed_action_sequence_playbacks
                {
                    if owner.0 == target_owner && id.id == target_node {
                        sequence.set_active_and_restart(*active);
                        if *active {
                            if let Ok(members) = children.get(entity) {
                                for child in members.iter() {
                                    let Ok((_, _, _, visibility, _, animation, _, _)) =
                                        nodes.get_mut(child)
                                    else {
                                        continue;
                                    };
                                    if let Some(mut animation) = animation {
                                        animation.start_authored_visibility_transition(false);
                                        wind_animation(&mut animation);
                                    }
                                    if let Some(mut visibility) = visibility {
                                        *visibility = Visibility::Hidden;
                                    }
                                }
                            }
                        }
                        break;
                    }
                }
            }
            UiPresentationAction::SetTargetNodeInteractionEnabled {
                target_node,
                enabled,
            } => {
                let target_node = openzt2_game_data::AssetId(target_node.0);
                if let Some((_, id, ..)) = nodes
                    .iter_mut()
                    .find(|(_, id, owner, ..)| owner.0 == source_owner.0 && id.id == target_node)
                {
                    enable_changes.write(SetAuthoredUiNodeInteractionEnabled::for_projected_node(
                        source_owner.0,
                        id.index,
                        *enabled,
                    ));
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
                let target = nodes
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
                        value.set_if_neq(UiValue(crate::slider::stepped_value(
                            value.0, delta, policy, minimum.0, maximum.0,
                        )));
                    } else if let Ok(mut position) = scroll_positions.get_mut(target) {
                        position.0 += Vec2::new(position_delta[0] as f32, position_delta[1] as f32);
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
                if let Some(target) = target_lookup
                    .candidates(source_owner.0, *target_node)
                    .next()
                {
                    let rectangle = UiSourceRect(*source_rectangle);
                    for entity in std::iter::once(target)
                        .chain(children.get(target).into_iter().flatten().copied())
                    {
                        if entity != target
                            && !image_nodes.get(entity).is_ok_and(|(_, is_layer)| is_layer)
                        {
                            continue;
                        }
                        if let Ok(mut source) = source_rectangles.get_mut(entity) {
                            source.set_if_neq(rectangle);
                        } else {
                            commands.entity(entity).insert(rectangle);
                        }
                    }
                }
            }
            UiPresentationAction::SetTargetNodeImageOverride {
                target_node,
                image_asset,
            } => {
                let Some(image) = document.cloned_texture_image_handle(*image_asset) else {
                    continue;
                };
                if let Some(target) = target_lookup
                    .candidates(source_owner.0, *target_node)
                    .next()
                {
                    let mut replaced = false;
                    for entity in std::iter::once(target)
                        .chain(children.get(target).into_iter().flatten().copied())
                    {
                        if let Some((node, _)) = image_nodes
                            .get_mut(entity)
                            .ok()
                            .filter(|(_, is_layer)| entity == target || *is_layer)
                        {
                            node.map_unchanged(|node| &mut node.image)
                                .set_if_neq(image.clone());
                            replaced = true;
                        }
                    }
                    if !replaced {
                        commands.entity(target).insert(ImageNode::new(image));
                    }
                }
            }
            UiPresentationAction::SetTargetNodeLayoutPosition {
                target_node,
                position,
            } => {
                let target_node = openzt2_game_data::AssetId(target_node.0);
                if let Some(target) = target_lookup.candidates(source_owner.0, target_node).next() {
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
                    if let Some((_, _, mut text)) = texts
                        .iter_mut()
                        .find(|(id, owner, _)| owner.0 == source_owner.0 && id.id == target_node)
                    {
                        text.0 = value;
                    }
                }
            }
            UiPresentationAction::FormatTargetNodeLocalizedInteger {
                target_node,
                localization_key,
                value,
            } => {
                if let (Some(localization), Some((_, _, mut text))) = (
                    localization,
                    texts
                        .iter_mut()
                        .find(|(id, owner, _)| owner.0 == source_owner.0 && id.id == *target_node),
                ) {
                    let _ = super::localized_ui_text_writing::write_localized_ui_text(
                        localization,
                        *localization_key,
                        &[
                            openzt2_game_data::localization::LocalizationFormatArgument::Integer(
                                *value,
                            ),
                        ],
                        &mut text.0,
                    );
                }
            }
            UiPresentationAction::SetTargetNodeTextFromLocalizationKey {
                target_node,
                localization_key,
            } => {
                let target_node = openzt2_game_data::AssetId(target_node.0);
                let localization_key = openzt2_game_data::AssetId(localization_key.0);
                if let Some(value) = localization.and_then(|localization| {
                    crate::localized_ui_text_writing::localized_ui_text(
                        localization,
                        localization_key,
                    )
                }) {
                    if let Some((_, _, mut text)) = texts
                        .iter_mut()
                        .find(|(id, owner, _)| owner.0 == source_owner.0 && id.id == target_node)
                    {
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
                if let Some(target) = target_lookup.candidates(source_owner.0, target_node).next() {
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
                if let Some(target) = target_lookup.candidates(source_owner.0, target_node).next() {
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
                if let Some(target) = target_lookup.candidates(source_owner.0, target_node).next() {
                    if let Ok(mut layout) = layout_nodes.get_mut(target) {
                        layout.width = Val::Px(size[0] as f32);
                        layout.height = Val::Px(size[1] as f32);
                    }
                }
            }
            UiPresentationAction::SetTargetNodeModal { target_node, modal } => {
                let target_node = openzt2_game_data::AssetId(target_node.0);
                if let Some(target) = target_lookup.candidates(source_owner.0, target_node).next() {
                    let next = UiAuthoredModalPresentation::from_authored_modal_state(*modal);
                    if let Ok(mut modal) = modals.get_mut(target) {
                        modal.set_if_neq(next);
                    } else {
                        commands.entity(target).insert(next);
                    }
                }
            }
            UiPresentationAction::SetTargetNodeTooltip {
                target_node,
                localization_key,
                long_tooltip,
            } => {
                let target_node = openzt2_game_data::AssetId(target_node.0);
                if let Some(target) = target_lookup.candidates(source_owner.0, target_node).next() {
                    let localization_key = openzt2_game_data::AssetId(localization_key.0);
                    if *long_tooltip {
                        let next = UiLongTooltipKey(localization_key);
                        if let Ok(mut tooltip) = long_tooltips.get_mut(target) {
                            tooltip.set_if_neq(next);
                        } else {
                            commands.entity(target).insert(next);
                        }
                    } else {
                        let next = UiShortTooltipKey(localization_key);
                        if let Ok(mut tooltip) = short_tooltips.get_mut(target) {
                            tooltip.set_if_neq(next);
                        } else {
                            commands.entity(target).insert(next);
                        }
                    }
                }
            }
            UiPresentationAction::SetTargetNodeCursor {
                target_node,
                cursor_asset,
            } => {
                let target_node = openzt2_game_data::AssetId(target_node.0);
                if let Some(target) = target_lookup.candidates(source_owner.0, target_node).next() {
                    let next = UiCursorStyle(*cursor_asset);
                    if let Ok(mut cursor) = cursors.get_mut(target) {
                        cursor.set_if_neq(next);
                    } else {
                        commands.entity(target).insert(next);
                    }
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
                if let Some(target) = target_lookup.candidates(source_owner.0, target_node).next() {
                    // Programmatic emphasis leaves the pointer hit target intact.
                    use super::authored_ui_focus_navigation_and_activation::AuthoredHoverPresentation;
                    let mut target = commands.entity(target);
                    target.insert(UiFocusPresentation(*hover_presented));
                    if *hover_presented {
                        target.insert(AuthoredHoverPresentation);
                    } else {
                        target.remove::<AuthoredHoverPresentation>();
                    }
                }
            }
            UiPresentationAction::ActivateSelectedChildOfTargetNode {
                target_node,
                target_role,
            } => {
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
                    forwarded.push(UiNodeActivated {
                        source,
                        node: selected_child,
                        trigger: UiTrigger::On,
                    });
                } else if let Some(role) = target_role.filter(|_| target.is_none()) {
                    document_operations.write(AuthoredUiDocumentOperation {
                        owner: lifecycle_owner.parent(),
                        role,
                        operation: UiDocumentOperation::ActivateSelectedChild {
                            target: target_node,
                            source: activation.source,
                        },
                        transfer_pause: false,
                    });
                }
            }
            UiPresentationAction::FocusPreviousOrNextSiblingOfTargetNode {
                target_node,
                focus_next,
            } => {
                let target_node = openzt2_game_data::AssetId(target_node.0);
                let target = target_lookup.candidates(source_owner.0, target_node).next();
                if let Some(target) = target.filter(|target| toggle_groups.contains(*target)) {
                    let members = children
                        .get(target)
                        .into_iter()
                        .flat_map(|children| children.iter())
                        .filter_map(|child| {
                            let (_, _, _, _, _, _, selected, _) = nodes.get_mut(child).ok()?;
                            let selected = selected?.0;
                            focusables.get(child).ok().and_then(|(_, policy, _)| {
                                policy.enabled.then_some((child, selected))
                            })
                        })
                        .collect::<Vec<_>>();
                    if !members.is_empty() {
                        let current = members.iter().position(|(_, selected)| *selected);
                        let index = match (current, *focus_next) {
                            (Some(index), true) => (index + 1) % members.len(),
                            (Some(index), false) => (index + members.len() - 1) % members.len(),
                            (None, true) => 0,
                            (None, false) => members.len() - 1,
                        };
                        let node = members[index].0;
                        let source = activation.source;
                        forwarded.push(UiNodeActivated {
                            source,
                            node,
                            trigger: UiTrigger::Press,
                        });
                    }
                    continue;
                }
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
                let parent = target_lookup.candidates(source_owner.0, target_node).next();
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
    let _ = activations.p1().write_batch(forwarded);
}

fn wind_animation(animation: &mut crate::animation::UiShowHideAnimation) {
    animation.elapsed_ms = if animation.forward {
        animation.duration_ms
    } else {
        0.0
    };
    animation.running = true;
}
