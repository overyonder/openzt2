use super::{
    AudioOperation, AudioPresentation, CachedMessageOperation, CachedMessagePresentation,
    DocumentOperation, DocumentPresentation, DrawingOperation, DrawingPresentation,
    InteractionOperation, InteractionPresentation, SelectionOperation, SelectionPresentation,
    TextOperation, TextPresentation, ValuesOperation, ValuesPresentation, VisibilityOperation,
    VisibilityPresentation,
};
use bevy::{
    input_focus::{FocusCause, InputFocus},
    prelude::*,
};
use openzt2_game_assets::{
    localization::{
        localization_asset_types::LocalizationAsset,
        localization_precedence_index::LocalizationPrecedenceIndex,
    },
    ui_document::ui_document_asset_types_and_borrowing_queries::UiDocumentAsset,
};
use openzt2_game_data::{localization::LocalizationFormatArgument, ui_document::action::UiTrigger};
use openzt2_interface_state::ui::{
    authored_ui_activation_contracts::UiNodeActivated,
    authored_ui_focus_state::UiFocusable,
    authored_ui_integer_range_components::{UiMaximum, UiMinimum},
    authored_ui_interaction_enabled_state::UiInteractionEnabled,
    authored_ui_node_projection_components::{
        UiDocumentMembers, UiDocumentOwner, UiDocumentRoot, UiNodeId, UiValue,
    },
    authored_ui_selection_state::UiSelected,
    authored_ui_visual_types::{UiSourceRect, UiVisualLayer},
    pointer_capture_types::UiPointerCapture,
    ui_document_lifecycle_contracts::{AuthoredUiDocumentOperation, UiDocumentOperation},
};
use openzt2_world_state::simulation_time::simulation_control_types::SimulationControl;
use openzt2_zoo_state::audio::audio_playback_message_types::PlayAudioClip;

use crate::authored_ui_change_activation_dispatch::set_authored_visibility;

use crate::{
    animation::UiShowHideAnimation,
    authored_modal_presentation::UiAuthoredModalPresentation,
    authored_reusable_list_and_table_runtime_types::UiTypeListPolicy,
    authored_timed_action_sequence::UiAuthoredTimedActionSequencePlayback,
    authored_toggle_selection_transitions::UiAuthoredToggleGroupSelectionPolicy,
    authored_tooltip_presentation::{UiLongTooltipKey, UiShortTooltipKey},
    authored_tree_expansion_and_row_indentation::UiAuthoredTreeExpansion,
    authored_ui_change_activation_dispatch::apply_visibility_transition,
    authored_ui_focus_navigation_and_activation::{
        choose_wrapped_authored_ui_focus_candidate, AuthoredHoverPresentation, UiFocusMemory,
    },
    authored_ui_target_lookup::AuthoredUiTargetLookup,
    cursor::UiCursorStyle,
    slider::{stepped_value, UiSliderAxis, UiSliderPolicy},
};

use super::super::{EmotePresentation, UiCachedMessages, UiExclusiveHoverPresentation};

pub(super) fn apply_document_presentation(
    event: On<DocumentPresentation>,
    controls: Query<&SimulationControl>,
    mut operations: MessageWriter<AuthoredUiDocumentOperation>,
    mut documents: crate::projection::ui_document_opening::LiveUiDocumentEffects,
) {
    let transfer_pause = controls
        .single()
        .ok()
        .is_some_and(SimulationControl::temporary_ui_pause_is_held);
    let (role, operation, transfer_pause) = match event.operation {
        DocumentOperation::ShowDocumentRole { document_role } => {
            (document_role, UiDocumentOperation::Open, transfer_pause)
        }
        DocumentOperation::SetDocumentNodeVisible {
            document_role,
            target_node,
            visible,
        } => (
            document_role,
            UiDocumentOperation::ShowNode {
                target: target_node,
                visible,
            },
            transfer_pause,
        ),
        DocumentOperation::HideDocumentRole { document_role } => {
            (document_role, UiDocumentOperation::Hide, false)
        }
        DocumentOperation::HideOwningDocument => {
            documents.apply(event.context.document, UiDocumentOperation::Hide);
            return;
        }
    };
    if let Some(document) = documents.document_of_role(event.context.lifecycle_owner, role) {
        documents.apply(document, operation);
        return;
    }
    operations.write(AuthoredUiDocumentOperation {
        owner: event.context.lifecycle_owner,
        role,
        operation,
        transfer_pause,
    });
}

pub(super) fn apply_visibility_presentation(
    event: On<VisibilityPresentation>,
    targets: AuthoredUiTargetLookup,
    pointer: Res<UiPointerCapture>,
    parents: Query<&ChildOf>,
    members: Query<&UiDocumentMembers>,
    children: Query<&Children>,
    exclusives: Query<&UiNodeId, With<UiExclusiveHoverPresentation>>,
    mut visibility: Query<&mut Visibility>,
    mut animations: Query<&mut UiShowHideAnimation>,
    emotes: Query<Entity, (With<EmotePresentation>, Without<UiNodeId>)>,
    authored: Query<(&UiDocumentOwner, &UiNodeId)>,
    mut activated: MessageWriter<UiNodeActivated>,
) {
    let document = event.context.document;
    match event.operation {
        VisibilityOperation::SetTargetNodeVisible {
            target_node,
            visible,
        } => {
            let mut candidates = targets.candidates(document, target_node);
            let Some(target) = candidates.next() else {
                return;
            };
            if visible
                && (exclusives.contains(target) || candidates.any(|node| exclusives.contains(node)))
            {
                if let Ok(members) = members.get(document) {
                    for member in members.iter().filter(|entity| {
                        exclusives.get(*entity).is_ok_and(|id| id.id != target_node)
                    }) {
                        if let Ok(mut animation) = animations.get_mut(member) {
                            animation.forward = false;
                            animation.elapsed_ms = 0.0;
                            animation.delay_remaining_ms = 0.0;
                            animation.running = false;
                        }
                        if let Ok(mut visibility) = visibility.get_mut(member) {
                            apply_visibility_transition(
                                member,
                                &mut visibility,
                                Visibility::Hidden,
                                true,
                                &mut activated,
                            );
                        }
                    }
                }
            }
            let animated = if let Ok(mut animation) = animations.get_mut(target) {
                animation.start_authored_visibility_transition(visible);
                true
            } else {
                false
            };
            if visible || !animated {
                if let Ok(mut visibility) = visibility.get_mut(target) {
                    apply_visibility_transition(
                        target,
                        &mut visibility,
                        if visible {
                            Visibility::Inherited
                        } else {
                            Visibility::Hidden
                        },
                        true,
                        &mut activated,
                    );
                }
            }
        }
        VisibilityOperation::HideTargetNodeUnlessHovered { target_node } => {
            let Some(target) = targets.candidates(document, target_node).next() else {
                return;
            };
            if pointer.target.is_some_and(|hovered| {
                hovered == target
                    || parents
                        .iter_ancestors::<ChildOf>(hovered)
                        .any(|entity| entity == target)
            }) {
                return;
            }
            if let Ok(mut animation) = animations.get_mut(target) {
                animation.start_authored_visibility_transition(false);
            } else if let Ok(mut visibility) = visibility.get_mut(target) {
                apply_visibility_transition(
                    target,
                    &mut visibility,
                    Visibility::Hidden,
                    true,
                    &mut activated,
                );
            }
        }
        VisibilityOperation::WindTargetShowHideAnimationToCurrentDirectionBoundary {
            target_node,
        } => {
            if let Some(target) = targets.candidates(document, target_node).next() {
                if let Ok(mut animation) = animations.get_mut(target) {
                    wind_animation(&mut animation);
                }
            }
        }
        VisibilityOperation::ShowOnlyNamedChildOfTargetNode {
            target_node,
            child_node,
        } => {
            let Some(target) = targets.candidates(document, target_node).next() else {
                return;
            };
            let Ok(children) = children.get(target) else {
                return;
            };
            for entity in children.iter() {
                let Ok((owner, id)) = authored.get(entity) else {
                    continue;
                };
                if owner.0 != document {
                    continue;
                }
                if let Ok(mut visibility) = visibility.get_mut(entity) {
                    apply_visibility_transition(
                        entity,
                        &mut visibility,
                        if id.id == child_node {
                            Visibility::Inherited
                        } else {
                            Visibility::Hidden
                        },
                        true,
                        &mut activated,
                    );
                }
            }
        }
        VisibilityOperation::SetAllEmotePresentationsVisible { visible } => {
            let next = if visible {
                Visibility::Inherited
            } else {
                Visibility::Hidden
            };
            let mut current = visibility.iter_many_mut(&emotes);
            while let Some(visibility) = current.fetch_next() {
                if let Ok(mut visibility) = visibility {
                    visibility.set_if_neq(next);
                }
            }
        }
    }
}

pub(super) fn apply_interaction_presentation(
    event: On<InteractionPresentation>,
    targets: AuthoredUiTargetLookup,
    visibility: Query<&InheritedVisibility>,
    mut enabled: Query<&mut UiInteractionEnabled>,
    mut focusable: Query<&mut UiFocusable>,
    mut modals: Query<&mut UiAuthoredModalPresentation>,
    mut long_tooltips: Query<&mut UiLongTooltipKey>,
    mut short_tooltips: Query<&mut UiShortTooltipKey>,
    mut cursors: Query<&mut UiCursorStyle>,
    mut hover: Query<&mut AuthoredHoverPresentation>,
    mut expanded: Query<&mut UiAuthoredTreeExpansion>,
    mut activated: MessageWriter<UiNodeActivated>,
    mut commands: Commands,
) {
    let candidates = |target| targets.candidates(event.context.document, target);
    match event.operation {
        InteractionOperation::ActivateTargetNodeWithPress { target_node } => {
            let Some(node) = candidates(target_node)
                .find(|entity| visibility.get(*entity).is_ok_and(|visible| visible.get()))
                .or_else(|| candidates(target_node).next())
            else {
                return;
            };
            activated.write(UiNodeActivated {
                node,
                source: event.context.source,
                trigger: UiTrigger::Press,
            });
        }
        InteractionOperation::SetTargetNodeInteractionEnabled {
            target_node,
            enabled: next,
        } => {
            if let Some(node) = candidates(target_node).next() {
                if let Ok(mut enabled) = enabled.get_mut(node) {
                    enabled.set_if_neq(UiInteractionEnabled(next));
                }
                if let Ok(focusable) = focusable.get_mut(node) {
                    focusable
                        .map_unchanged(|policy| &mut policy.enabled)
                        .set_if_neq(next);
                }
            }
        }
        InteractionOperation::SetTargetNodeModal { target_node, modal } => {
            if let Some(node) = candidates(target_node).next() {
                let next = UiAuthoredModalPresentation::from_authored_modal_state(modal);
                if let Ok(mut current) = modals.get_mut(node) {
                    current.set_if_neq(next);
                } else {
                    commands.entity(node).insert(next);
                }
            }
        }
        InteractionOperation::SetTargetNodeTooltip {
            target_node,
            localization_key,
            long_tooltip,
        } => {
            if let Some(node) = candidates(target_node).next() {
                if long_tooltip {
                    let next = UiLongTooltipKey(localization_key);
                    if let Ok(mut current) = long_tooltips.get_mut(node) {
                        current.set_if_neq(next);
                    } else {
                        commands.entity(node).insert(next);
                    }
                } else {
                    let next = UiShortTooltipKey(localization_key);
                    if let Ok(mut current) = short_tooltips.get_mut(node) {
                        current.set_if_neq(next);
                    } else {
                        commands.entity(node).insert(next);
                    }
                }
            }
        }
        InteractionOperation::SetTargetNodeCursor {
            target_node,
            cursor_asset,
        } => {
            if let Some(node) = candidates(target_node).next() {
                let next = UiCursorStyle(cursor_asset);
                if let Ok(mut current) = cursors.get_mut(node) {
                    current.set_if_neq(next);
                } else {
                    commands.entity(node).insert(next);
                }
            }
        }
        InteractionOperation::SetTargetNodeHoverPresentation {
            target_node,
            hover_presented,
        } => {
            if let Some(node) = candidates(target_node).next() {
                if let Ok(mut current) = hover.get_mut(node) {
                    current.set_if_neq(AuthoredHoverPresentation(hover_presented));
                } else {
                    commands.entity(node).insert((
                        AuthoredHoverPresentation(hover_presented),
                        bevy::picking::hover::DirectlyHovered::default(),
                    ));
                }
            }
        }
        InteractionOperation::SetTargetNodeExpanded {
            target_node,
            expanded: next,
        } => {
            if let Some(node) = candidates(target_node).next() {
                if let Ok(mut current) = expanded.get_mut(node) {
                    current.set_expanded(next);
                } else {
                    commands
                        .entity(node)
                        .insert(UiAuthoredTreeExpansion::from_authored_expansion(next));
                }
            }
        }
    }
}

pub(super) fn apply_values_presentation(
    event: On<ValuesPresentation>,
    targets: AuthoredUiTargetLookup,
    mut values: Query<&mut UiValue>,
    mut scrolls: Query<&mut ScrollPosition>,
    scroll_geometry: Query<&ComputedNode>,
    scrollbars: Query<(&crate::authored_scrollbar::ScrollsViewport, &UiSliderPolicy)>,
    slider_bounds: Query<(&UiSliderPolicy, &UiMinimum, &UiMaximum)>,
    mut commands: Commands,
) {
    let document = event.context.document;
    match event.operation {
        ValuesOperation::SetTargetNodeIntegerValue {
            target_node,
            integer_value,
        } => {
            if let Some(entity) = targets.candidates(document, target_node).next() {
                if let Ok((scrollbar, policy)) = scrollbars.get(entity) {
                    if let (Ok(mut position), Ok(bounds)) = (
                        scrolls.get_mut(scrollbar.0),
                        scroll_geometry.get(scrollbar.0),
                    ) {
                        crate::authored_scrollbar::set_offset(
                            &mut position,
                            policy.axis,
                            integer_value as f32,
                            crate::authored_scrollbar::overflow(bounds),
                        );
                    }
                    return;
                }
                let next = UiValue(i64::from(integer_value));
                if let Ok(mut value) = values.get_mut(entity) {
                    value.set_if_neq(next);
                } else {
                    commands.entity(entity).insert(next);
                }
            }
        }
        ValuesOperation::ChangeTargetNodeSliderOrScrollPositionByDelta {
            target_node,
            position_delta,
        } => {
            let Some(entity) = targets.candidates(document, target_node).next() else {
                return;
            };
            if let Ok((scrollbar, policy)) = scrollbars.get(entity) {
                if let (Ok(mut position), Ok(bounds)) = (
                    scrolls.get_mut(scrollbar.0),
                    scroll_geometry.get(scrollbar.0),
                ) {
                    let delta = match policy.axis {
                        UiSliderAxis::Horizontal | UiSliderAxis::Both => position_delta[0],
                        UiSliderAxis::Vertical => position_delta[1],
                    };
                    let current =
                        crate::authored_scrollbar::offset(&position, policy.axis).round() as i64;
                    let next = stepped_value(
                        current,
                        delta,
                        policy,
                        0,
                        crate::authored_scrollbar::limit(bounds, policy.axis).round() as i64,
                    );
                    crate::authored_scrollbar::set_offset(
                        &mut position,
                        policy.axis,
                        next as f32,
                        crate::authored_scrollbar::overflow(bounds),
                    );
                }
                return;
            }
            if let (Ok(mut value), Ok((policy, lower, upper))) =
                (values.get_mut(entity), slider_bounds.get(entity))
            {
                let delta = match policy.axis {
                    UiSliderAxis::Horizontal | UiSliderAxis::Both => position_delta[0],
                    UiSliderAxis::Vertical => position_delta[1],
                };
                let next = stepped_value(value.0, delta, policy, lower.0, upper.0);
                value.set_if_neq(UiValue(next));
            } else {
                let delta = Vec2::new(position_delta[0] as f32, position_delta[1] as f32);
                if let Ok(mut position) = scrolls.get_mut(entity) {
                    position.0 += delta;
                } else {
                    commands.entity(entity).insert(ScrollPosition(delta));
                }
            }
        }
    }
}

pub(super) fn apply_drawing_presentation(
    event: On<DrawingPresentation>,
    targets: AuthoredUiTargetLookup,
    roots: Query<&UiDocumentRoot>,
    documents: Res<Assets<UiDocumentAsset>>,
    mut commands: Commands,
    children: Query<&Children>,
    mut images: Query<(&mut ImageNode, Has<UiVisualLayer>)>,
    mut rectangles: Query<&mut UiSourceRect>,
    mut layouts: Query<&mut Node>,
) {
    let document = event.context.document;
    match event.operation {
        DrawingOperation::SetTargetNodeImageSourceRectangle {
            target_node,
            source_rectangle,
        } => {
            let Some(target) = targets.candidates(document, target_node).next() else {
                return;
            };
            let next = UiSourceRect(source_rectangle);
            for entity in
                std::iter::once(target).chain(children.get(target).into_iter().flatten().copied())
            {
                if entity != target && !images.get(entity).is_ok_and(|(_, layer)| layer) {
                    continue;
                }
                if let Ok(mut current) = rectangles.get_mut(entity) {
                    current.set_if_neq(next);
                } else {
                    commands.entity(entity).insert(next);
                }
            }
        }
        DrawingOperation::SetTargetNodeImageOverride {
            target_node,
            image_asset,
        } => {
            let Some(image) = roots
                .get(document)
                .ok()
                .and_then(|root| documents.get(&root.document))
                .and_then(|document| document.cloned_texture_image_handle(image_asset))
            else {
                return;
            };
            let Some(target) = targets.candidates(document, target_node).next() else {
                return;
            };
            let mut found = false;
            for entity in
                std::iter::once(target).chain(children.get(target).into_iter().flatten().copied())
            {
                if let Ok((node, layer)) = images.get_mut(entity) {
                    if entity == target || layer {
                        node.map_unchanged(|image| &mut image.image)
                            .set_if_neq(image.clone());
                        found = true;
                    }
                }
            }
            if !found {
                commands.entity(target).insert(ImageNode::new(image));
            }
        }
        DrawingOperation::SetTargetNodeLayoutPosition {
            target_node,
            position,
        } => {
            if let Some(node) = targets.candidates(document, target_node).next() {
                if let Ok(mut layout) = layouts.get_mut(node) {
                    layout.left = px(position[0] as f32);
                    layout.top = px(position[1] as f32);
                }
            }
        }
        DrawingOperation::SetTargetNodeLayoutSize { target_node, size } => {
            if let Some(node) = targets.candidates(document, target_node).next() {
                if let Ok(mut layout) = layouts.get_mut(node) {
                    layout.width = px(size[0] as f32);
                    layout.height = px(size[1] as f32);
                }
            }
        }
    }
}

pub(super) fn apply_text_presentation(
    event: On<TextPresentation>,
    targets: AuthoredUiTargetLookup,
    localizations: Res<Assets<LocalizationAsset>>,
    precedence: Res<LocalizationPrecedenceIndex>,
    mut texts: Query<&mut Text>,
) {
    let candidates = |target| targets.candidates(event.context.document, target);
    let (target, value) = match &event.operation {
        TextOperation::CopySourceNodeTextToTargetNode {
            target_node,
            source_node,
        } => {
            let value = candidates(*source_node)
                .find_map(|entity| texts.get(entity).ok())
                .map(|text| text.0.clone());
            (*target_node, value)
        }
        TextOperation::SetTargetNodeTextFromLocalizationKey {
            target_node,
            localization_key,
        } => {
            let value = precedence
                .borrow_loaded_localization_view(&localizations)
                .and_then(|view| {
                    crate::localized_ui_text_writing::localized_ui_text(view, *localization_key)
                })
                .map(str::to_owned);
            (*target_node, value)
        }
        TextOperation::SetTargetNodeTextLiteral { target_node, text } => {
            (*target_node, Some(text.clone()))
        }
        TextOperation::FormatTargetNodeLocalizedInteger {
            target_node,
            localization_key,
            value,
        } => {
            if let Some(view) = precedence.borrow_loaded_localization_view(&localizations) {
                if let Some(entity) =
                    candidates(*target_node).find(|entity| texts.contains(*entity))
                {
                    if let Ok(mut text) = texts.get_mut(entity) {
                        let _ = crate::localized_ui_text_writing::write_localized_ui_text(
                            view,
                            *localization_key,
                            &[LocalizationFormatArgument::Integer(*value)],
                            &mut text.0,
                        );
                    }
                }
            }
            return;
        }
    };
    if let (Some(entity), Some(value)) = (
        candidates(target).find(|entity| texts.contains(*entity)),
        value,
    ) {
        if let Ok(mut text) = texts.get_mut(entity) {
            if text.0 != value {
                text.0 = value;
            }
        }
    }
}

pub(super) fn apply_audio_presentation(
    event: On<AudioPresentation>,
    roots: Query<&UiDocumentRoot>,
    documents: Res<Assets<UiDocumentAsset>>,
    mut sounds: MessageWriter<PlayAudioClip>,
) {
    let AudioOperation::PlayDocumentAudioCue { audio_cue } = event.operation;
    if let Some(clip) = roots
        .get(event.context.document)
        .ok()
        .and_then(|root| documents.get(&root.document))
        .and_then(|document| document.audio_source_handle(audio_cue))
    {
        sounds.write(PlayAudioClip {
            clip: clip.clone(),
            emitter: None,
            selection: 0,
            force_looped: false,
        });
    }
}

pub(super) fn apply_cachedmessage_presentation(
    event: On<CachedMessagePresentation>,
    targets: AuthoredUiTargetLookup,
    children: Query<&Children>,
    mut sequences: Query<&mut UiCachedMessages>,
    mut texts: Query<&mut Text>,
) {
    let CachedMessageOperation::AdvanceTargetNodeMessageCursor { target_node } = event.operation;
    let Some(entity) = targets
        .candidates(event.context.document, target_node)
        .next()
    else {
        return;
    };
    let Ok(mut sequence) = sequences.get_mut(entity) else {
        return;
    };
    let Some(value) = sequence.messages.get(sequence.next) else {
        return;
    };
    let text = std::iter::once(entity)
        .chain(children.iter_descendants::<Children>(entity))
        .find(|node| texts.contains(*node));
    if let Some(text) = text {
        if let Ok(mut text) = texts.get_mut(text) {
            if text.0 != *value {
                text.0.clone_from(value);
            }
            sequence.next += 1;
        }
    }
}

fn wind_animation(animation: &mut UiShowHideAnimation) {
    animation.elapsed_ms = if animation.forward {
        animation.duration_ms
    } else {
        0.0
    };
    animation.running = true;
}

pub(super) fn apply_selection_presentation(
    event: On<SelectionPresentation>,
    targets: AuthoredUiTargetLookup,
    documents: Res<Assets<UiDocumentAsset>>,
    roots: Query<&UiDocumentRoot>,
    children: Query<&Children>,
    parents: Query<&ChildOf>,
    groups: Query<(), With<UiAuthoredToggleGroupSelectionPolicy>>,
    members: Query<&UiDocumentMembers>,
    owners: Query<&UiDocumentOwner>,
    mut selected: Query<(&mut UiSelected, Has<UiTypeListPolicy>)>,
    timelines: (
        Query<&mut UiAuthoredTimedActionSequencePlayback>,
        Query<&mut UiShowHideAnimation>,
        Query<&mut Visibility>,
    ),
    focusables: Query<&UiFocusable>,
    mut focus: ResMut<InputFocus>,
    mut memory: Query<&mut UiFocusMemory>,
    mut operations: MessageWriter<AuthoredUiDocumentOperation>,
    mut activated: MessageWriter<UiNodeActivated>,
) {
    let (mut playback, mut animations, mut visibility) = timelines;
    let context = event.context;
    let role_documents = || {
        children
            .get(context.lifecycle_owner)
            .into_iter()
            .flatten()
            .copied()
    };
    let mut emit = |node, trigger| {
        activated.write(UiNodeActivated {
            node,
            source: context.source,
            trigger,
        });
    };
    match event.operation {
        SelectionOperation::SetTargetNodeSelectionAndTimedSequencesActive {
            document_role,
            target_node,
            active,
        } => {
            let document = if let Some(role) = document_role {
                let Some(root) = role_documents().find(|entity| {
                    roots
                        .get(*entity)
                        .ok()
                        .and_then(|root| documents.get(&root.document))
                        .is_some_and(|asset| asset.canonical_ui_document().role == role)
                }) else {
                    return;
                };
                root
            } else {
                context.document
            };
            let candidates = || targets.candidates(document, target_node);
            let Some(target) = candidates().next() else {
                return;
            };
            let mut set_selected = |entity, next| {
                if let Ok((mut current, is_type_list)) = selected.get_mut(entity) {
                    let changed = current.set_if_neq(UiSelected(next));
                    if changed || is_type_list && next {
                        emit(entity, if next { UiTrigger::On } else { UiTrigger::Off });
                    }
                }
            };
            if active {
                if let Ok(parent) = parents.get(target) {
                    if groups.contains(parent.parent()) {
                        if let Ok(siblings) = children.get(parent.parent()) {
                            for sibling in siblings.iter().filter(|entity| *entity != target) {
                                set_selected(sibling, false);
                            }
                        }
                    }
                }
            }
            for node in candidates() {
                set_selected(node, active);
                if !active && groups.contains(node) {
                    if let Ok(children) = children.get(node) {
                        for member in children.iter() {
                            set_selected(member, false);
                        }
                    }
                }
            }
            if let Some(entity) = candidates().find(|entity| playback.contains(*entity)) {
                if let Ok(mut timeline) = playback.get_mut(entity) {
                    timeline.set_active_and_restart(active);
                    if active {
                        if let Ok(children) = children.get(entity) {
                            for child in children.iter() {
                                if let Ok(mut animation) = animations.get_mut(child) {
                                    animation.start_authored_visibility_transition(false);
                                    wind_animation(&mut animation);
                                }
                                if let Ok(mut visibility) = visibility.get_mut(child) {
                                    if let Some(event) = set_authored_visibility(
                                        child,
                                        &mut visibility,
                                        Visibility::Hidden,
                                        owners.contains(child),
                                    ) {
                                        activated.write(event);
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        SelectionOperation::SelectTargetNode { target_node } => {
            if let Some(node) = targets.candidates(context.document, target_node).next() {
                if let Ok((mut selected, _)) = selected.get_mut(node) {
                    if selected.set_if_neq(UiSelected(true)) {
                        emit(node, UiTrigger::On);
                    }
                }
            }
        }
        SelectionOperation::ActivateSelectedChildOfTargetNode {
            target_node,
            target_role,
        } => {
            let target = std::iter::once(context.document)
                .chain(role_documents())
                .find_map(|document| {
                    targets
                        .candidates(document, target_node)
                        .next()
                        .map(|node| (document, node))
                });
            if let Some((document, parent)) = target {
                if let Some(node) =
                    children
                        .get(parent)
                        .into_iter()
                        .flatten()
                        .copied()
                        .find(|entity| {
                            owners.get(*entity).is_ok_and(|owner| owner.0 == document)
                                && selected.get(*entity).is_ok_and(|(selected, _)| selected.0)
                        })
                {
                    emit(node, UiTrigger::On);
                }
            } else if let Some(role) = target_role {
                operations.write(AuthoredUiDocumentOperation {
                    owner: context.lifecycle_owner,
                    role,
                    operation: UiDocumentOperation::ActivateSelectedChild {
                        target: target_node,
                        source: context.source,
                    },
                    transfer_pause: false,
                });
            }
        }
        SelectionOperation::FocusPreviousOrNextSiblingOfTargetNode {
            target_node,
            focus_next,
        } => {
            let Some(target) = targets.candidates(context.document, target_node).next() else {
                return;
            };
            if groups.contains(target) {
                let group = children
                    .get(target)
                    .into_iter()
                    .flatten()
                    .copied()
                    .filter(|entity| focusables.get(*entity).is_ok_and(|policy| policy.enabled))
                    .filter_map(|entity| {
                        selected
                            .get(entity)
                            .ok()
                            .map(|(selected, _)| (entity, selected.0))
                    })
                    .collect::<Vec<_>>();
                if group.is_empty() {
                    return;
                }
                let current = group.iter().position(|(_, selected)| *selected);
                let index = match (current, focus_next) {
                    (Some(index), true) => (index + 1) % group.len(),
                    (Some(index), false) => (index + group.len() - 1) % group.len(),
                    (None, true) => 0,
                    (None, false) => group.len() - 1,
                };
                emit(group[index].0, UiTrigger::Press);
                return;
            }
            let Ok(policy) = focusables.get(target) else {
                return;
            };
            let Ok(members) = members.get(context.document) else {
                return;
            };
            let next = choose_wrapped_authored_ui_focus_candidate(
                Some(policy.order),
                if focus_next { 1 } else { -1 },
                members.iter().filter_map(|entity| {
                    let candidate = focusables.get(entity).ok()?;
                    candidate.enabled.then_some((candidate.order, entity))
                }),
            );
            if let Some(next) = next {
                if let Ok(mut memory) = memory.get_mut(context.document) {
                    memory.remembered = Some(next);
                }
                if focus.get() != Some(next) {
                    focus.set(next, FocusCause::Navigated);
                }
            }
        }
    }
}
