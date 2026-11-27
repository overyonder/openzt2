use super::PresentationContext;
use bevy::{
    ecs::system::SystemParam,
    input_focus::{tab_navigation::TabIndex, FocusCause, InputFocus},
    prelude::*,
};
use openzt2_game_assets::{
    localization::{
        localization_asset_types::LocalizationAsset,
        localization_precedence_index::LocalizationPrecedenceIndex,
    },
    ui_document::ui_document_asset_types_and_borrowing_queries::UiDocumentAsset,
};
use openzt2_game_data::ui_document::action::presentation::UiPresentationAction;
use openzt2_game_data::{localization::LocalizationFormatArgument, ui_document::action::UiTrigger};
use openzt2_interface_state::ui::{
    authored_ui_activation_contracts::UiNodeActivated,
    authored_ui_focus_state::UiFocusable,
    authored_ui_integer_range_components::{UiMaximum, UiMinimum},
    authored_ui_interaction_enabled_state::UiInteractionEnabled,
    authored_ui_node_projection_components::{
        UiDocumentMembers, UiDocumentOwner, UiDocumentRoot, UiNodeId, UiSliderValueChanged, UiValue,
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
    animation::{UiAnimationPlaybackOperation, UiAnimationPlaybackRequest, UiShowHideAnimation},
    authored_modal_presentation::UiAuthoredModalPresentation,
    authored_reusable_list_and_table_runtime_types::UiTypeListPolicy,
    authored_timed_action_sequence::{UiAuthoredTimedActionSequencePlayback, UiSelectionWaitEnded},
    authored_toggle_selection_transitions::UiAuthoredToggleGroupSelectionPolicy,
    authored_tooltip_presentation::{UiLongTooltipKey, UiShortTooltipKey},
    authored_tree_expansion_and_row_indentation::UiAuthoredTreeExpansion,
    authored_ui_focus_navigation_and_activation::{
        choose_wrapped_authored_ui_focus_candidate, AuthoredHoverPresentation, UiFocusMemory,
    },
    authored_ui_target_lookup::AuthoredUiTargetLookup,
    cursor::UiCursorStyle,
    slider::{accept_slider_value, stepped_value, UiSliderAxis, UiSliderPolicy},
};

use super::super::{EmotePresentation, UiCachedMessages, UiExclusiveHoverPresentation};

fn record_visibility_transition(
    node: Entity,
    visibility: &mut Mut<Visibility>,
    next: Visibility,
    authored: bool,
    emitted: &mut Vec<UiNodeActivated>,
) {
    emitted.extend(set_authored_visibility(node, visibility, next, authored));
}

#[derive(SystemParam)]
pub(super) struct DocumentWrites<'w, 's> {
    controls: Query<'w, 's, &'static SimulationControl>,
    operations: MessageWriter<'w, AuthoredUiDocumentOperation>,
    documents: crate::projection::ui_document_opening::LiveUiDocumentEffects<'w, 's>,
    game_view:
        openzt2_interface_state::ui::ui_document_lifecycle_contracts::GameViewDocumentOwner<'w, 's>,
}

impl DocumentWrites<'_, '_> {
    pub(super) fn apply_opened_document_effect(
        &mut self,
        document: Entity,
        effect: UiDocumentOperation,
        emitted: &mut Vec<UiNodeActivated>,
    ) {
        self.documents
            .apply(document, effect, &mut |activation| emitted.push(activation));
    }

    pub(super) fn apply(
        &mut self,
        context: PresentationContext,
        action: UiPresentationAction,
        emitted: &mut Vec<UiNodeActivated>,
    ) {
        let Self {
            controls,
            operations,
            documents,
            game_view,
        } = self;
        let transfer_pause = controls
            .single()
            .ok()
            .is_some_and(SimulationControl::temporary_ui_pause_is_held);
        let (role, operation, transfer_pause) = match action {
            UiPresentationAction::ShowDocumentRole { document_role } => {
                (document_role, UiDocumentOperation::Open, transfer_pause)
            }
            UiPresentationAction::SetDocumentNodeVisible {
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
            UiPresentationAction::HideDocumentRole { document_role } => {
                (document_role, UiDocumentOperation::Hide, false)
            }
            UiPresentationAction::HideOwningDocument
            | UiPresentationAction::HideOwningDocumentAfterAlertAcknowledgement
            | UiPresentationAction::HideOwningDocumentAfterConfirmationDismissal => {
                documents.apply(
                    context.document,
                    UiDocumentOperation::Hide,
                    &mut |activation| emitted.push(activation),
                );
                return;
            }
            _ => unreachable!(),
        };
        // A document that hides as it opens another, such as the in-game
        // options opening the save prompt, must not take that one with it.
        let owner = game_view.owner(context.lifecycle_owner);
        if let Some(document) = documents.document_of_role(owner, role) {
            documents.apply(document, operation, &mut |activation| {
                emitted.push(activation)
            });
            return;
        }
        operations.write(AuthoredUiDocumentOperation {
            owner,
            role,
            operation,
            transfer_pause,
        });
    }
}

#[derive(SystemParam)]
pub(super) struct VisibilityWrites<'w, 's> {
    targets: AuthoredUiTargetLookup<'w, 's>,
    pointer: Res<'w, UiPointerCapture>,
    parents: Query<'w, 's, &'static ChildOf>,
    members: Query<'w, 's, &'static UiDocumentMembers>,
    children: Query<'w, 's, &'static Children>,
    exclusives: Query<'w, 's, &'static UiNodeId, With<UiExclusiveHoverPresentation>>,
    visibility: Query<'w, 's, &'static mut Visibility>,
    animations: Query<'w, 's, (), With<UiShowHideAnimation>>,
    pending_animation: MessageWriter<'w, UiAnimationPlaybackRequest>,
    emotes: Query<'w, 's, Entity, (With<EmotePresentation>, Without<UiNodeId>)>,
    authored: Query<'w, 's, (&'static UiDocumentOwner, &'static UiNodeId)>,
}

impl VisibilityWrites<'_, '_> {
    pub(super) fn apply(
        &mut self,
        context: PresentationContext,
        action: UiPresentationAction,
        emitted: &mut Vec<UiNodeActivated>,
    ) {
        let Self {
            targets,
            pointer,
            parents,
            members,
            children,
            exclusives,
            visibility,
            animations,
            pending_animation,
            emotes,
            authored,
        } = self;
        let document = context.document;
        match action {
            UiPresentationAction::SetTargetNodeVisible {
                target_node,
                visible,
            } => {
                let mut candidates = targets.candidates(document, target_node);
                let Some(target) = candidates.next() else {
                    return;
                };
                if visible
                    && (exclusives.contains(target)
                        || candidates.any(|node| exclusives.contains(node)))
                {
                    if let Ok(members) = members.get(document) {
                        for member in members.iter().filter(|entity| {
                            exclusives.get(*entity).is_ok_and(|id| id.id != target_node)
                        }) {
                            if animations.contains(member) {
                                pending_animation.write(UiAnimationPlaybackRequest {
                                    entity: member,
                                    operation: UiAnimationPlaybackOperation::Reset(false),
                                });
                            }
                            if let Ok(mut visibility) = visibility.get_mut(member) {
                                record_visibility_transition(
                                    member,
                                    &mut visibility,
                                    Visibility::Hidden,
                                    true,
                                    emitted,
                                );
                            }
                        }
                    }
                }
                let animated = animations.contains(target);
                if animated {
                    pending_animation.write(UiAnimationPlaybackRequest {
                        entity: target,
                        operation: UiAnimationPlaybackOperation::Transition(visible),
                    });
                }
                if visible || !animated {
                    if let Ok(mut visibility) = visibility.get_mut(target) {
                        record_visibility_transition(
                            target,
                            &mut visibility,
                            if visible {
                                Visibility::Inherited
                            } else {
                                Visibility::Hidden
                            },
                            true,
                            emitted,
                        );
                    }
                }
            }
            UiPresentationAction::HideTargetNodeUnlessHovered { target_node } => {
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
                if animations.contains(target) {
                    pending_animation.write(UiAnimationPlaybackRequest {
                        entity: target,
                        operation: UiAnimationPlaybackOperation::Transition(false),
                    });
                } else if let Ok(mut visibility) = visibility.get_mut(target) {
                    record_visibility_transition(
                        target,
                        &mut visibility,
                        Visibility::Hidden,
                        true,
                        emitted,
                    );
                }
            }
            UiPresentationAction::WindTargetShowHideAnimationToCurrentDirectionBoundary {
                target_node,
            } => {
                if let Some(target) = targets.candidates(document, target_node).next() {
                    if animations.contains(target) {
                        pending_animation.write(UiAnimationPlaybackRequest {
                            entity: target,
                            operation: UiAnimationPlaybackOperation::Finish,
                        });
                    }
                }
            }
            UiPresentationAction::ShowOnlyNamedChildOfTargetNode {
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
                        record_visibility_transition(
                            entity,
                            &mut visibility,
                            if id.id == child_node {
                                Visibility::Inherited
                            } else {
                                Visibility::Hidden
                            },
                            true,
                            emitted,
                        );
                    }
                }
            }
            UiPresentationAction::SetAllEmotePresentationsVisible { visible } => {
                let next = if visible {
                    Visibility::Inherited
                } else {
                    Visibility::Hidden
                };
                let mut current = visibility.iter_many_mut(emotes.iter());
                while let Some(visibility) = current.fetch_next() {
                    if let Ok(mut visibility) = visibility {
                        visibility.set_if_neq(next);
                    }
                }
            }
            _ => unreachable!(),
        }
    }
}

#[derive(SystemParam)]
pub(super) struct InteractionWrites<'w, 's> {
    targets: AuthoredUiTargetLookup<'w, 's>,
    visibility: Query<'w, 's, &'static InheritedVisibility>,
    edits: Query<'w, 's, &'static bevy::text::EditableText>,
    enabled: Query<'w, 's, &'static mut UiInteractionEnabled>,
    modals: Query<'w, 's, &'static mut UiAuthoredModalPresentation>,
    long_tooltips: Query<'w, 's, &'static mut UiLongTooltipKey>,
    short_tooltips: Query<'w, 's, &'static mut UiShortTooltipKey>,
    cursors: Query<'w, 's, &'static mut UiCursorStyle>,
    hover: Query<'w, 's, &'static mut AuthoredHoverPresentation>,
    expanded: Query<'w, 's, &'static mut UiAuthoredTreeExpansion>,
    commands: Commands<'w, 's>,
}

impl InteractionWrites<'_, '_> {
    pub(super) fn apply(
        &mut self,
        context: PresentationContext,
        action: UiPresentationAction,
        emitted: &mut Vec<UiNodeActivated>,
        missing: &mut bevy::ecs::entity::EntityHashMap<UiAuthoredTreeExpansion>,
    ) {
        let Self {
            targets,
            visibility,
            edits,
            enabled,
            modals,
            long_tooltips,
            short_tooltips,
            cursors,
            hover,
            expanded,
            commands,
        } = self;
        let candidates = |target| targets.candidates(context.document, target);
        match action {
            UiPresentationAction::WatchTextEdit {
                target_node,
                source_node,
            } => {
                let Some((source, edit)) = targets
                    .candidates(context.document, source_node)
                    .find_map(|source| edits.get(source).ok().map(|edit| (source, edit)))
                else {
                    return;
                };
                let value = UiInteractionEnabled(!edit.value().to_string().trim().is_empty());
                for target in targets.candidates(context.document, target_node) {
                    if let Ok(mut current) = enabled.get_mut(target) {
                        current.set_if_neq(value);
                        commands.entity(target).insert(
                            crate::authored_text_edit_bevy_adaptation::UiWatchesTextEdit(source),
                        );
                        break;
                    }
                }
            }
            UiPresentationAction::ActivateTargetNodeWithPress { target_node } => {
                let Some(node) = candidates(target_node)
                    .find(|entity| visibility.get(*entity).is_ok_and(|visible| visible.get()))
                    .or_else(|| candidates(target_node).next())
                else {
                    return;
                };
                emitted.push(UiNodeActivated {
                    node,
                    source: context.source,
                    trigger: UiTrigger::Press,
                });
            }
            UiPresentationAction::SetTargetNodeInteractionEnabled {
                target_node,
                enabled: next,
            } => {
                if let Some(node) = candidates(target_node).next() {
                    if let Ok(mut enabled) = enabled.get_mut(node) {
                        enabled.set_if_neq(UiInteractionEnabled(next));
                    }
                }
            }
            UiPresentationAction::SetTargetNodeModal { target_node, modal } => {
                if let Some(node) = candidates(target_node).next() {
                    let next = UiAuthoredModalPresentation::from_authored_modal_state(modal);
                    if let Ok(mut current) = modals.get_mut(node) {
                        current.set_if_neq(next);
                    } else {
                        commands.entity(node).insert(next);
                    }
                }
            }
            UiPresentationAction::SetTargetNodeTooltip {
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
            UiPresentationAction::SetTargetNodeCursor {
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
            UiPresentationAction::SetTargetNodeHoverPresentation {
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
            UiPresentationAction::SetTargetNodeExpanded {
                target_node,
                expanded: next,
            } => {
                if let Some(node) = candidates(target_node).next() {
                    if let Ok(mut current) = expanded.get_mut(node) {
                        current.set_expanded(next);
                    } else {
                        missing
                            .insert(node, UiAuthoredTreeExpansion::from_authored_expansion(next));
                    }
                }
            }
            _ => unreachable!(),
        }
    }
}

#[derive(SystemParam)]
pub(super) struct ValuesWrites<'w, 's> {
    targets: AuthoredUiTargetLookup<'w, 's>,
    values: Query<'w, 's, &'static mut UiValue>,
    scrolls: Query<'w, 's, &'static mut ScrollPosition>,
    scroll_geometry: Query<'w, 's, &'static ComputedNode>,
    scrollbars: Query<
        'w,
        's,
        (
            &'static crate::authored_scrollbar::ScrollsViewport,
            &'static UiSliderPolicy,
        ),
    >,
    slider_bounds: Query<
        'w,
        's,
        (
            &'static UiSliderPolicy,
            &'static UiMinimum,
            &'static UiMaximum,
        ),
    >,
    slider_edits: MessageWriter<'w, UiSliderValueChanged>,
    commands: Commands<'w, 's>,
}

impl ValuesWrites<'_, '_> {
    pub(super) fn apply(
        &mut self,
        context: PresentationContext,
        action: UiPresentationAction,
        missing: &mut bevy::ecs::entity::EntityHashMap<ScrollPosition>,
    ) {
        let Self {
            targets,
            values,
            scrolls,
            scroll_geometry,
            scrollbars,
            slider_bounds,
            slider_edits,
            commands,
        } = self;
        let document = context.document;
        match action {
            UiPresentationAction::SetTargetNodeIntegerValue {
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
            UiPresentationAction::ChangeTargetNodeSliderOrScrollPositionByDelta {
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
                        let current = crate::authored_scrollbar::offset(&position, policy.axis)
                            .round() as i64;
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
                        UiSliderAxis::Horizontal => position_delta[0],
                        UiSliderAxis::Vertical => position_delta[1],
                        UiSliderAxis::Both => 0,
                    };
                    let next = stepped_value(value.0, delta, policy, lower.0, upper.0);
                    if delta != 0 {
                        accept_slider_value(entity, &mut value, next, slider_edits);
                    }
                } else {
                    let delta = Vec2::new(position_delta[0] as f32, position_delta[1] as f32);
                    if let Ok(mut position) = scrolls.get_mut(entity) {
                        position.0 += delta;
                    } else {
                        missing.entry(entity).or_default().0 += delta;
                    }
                }
            }
            _ => unreachable!(),
        }
    }
}

#[derive(SystemParam)]
pub(super) struct DrawingWrites<'w, 's> {
    targets: AuthoredUiTargetLookup<'w, 's>,
    roots: Query<'w, 's, &'static UiDocumentRoot>,
    documents: Res<'w, Assets<UiDocumentAsset>>,
    commands: Commands<'w, 's>,
    children: Query<'w, 's, &'static Children>,
    images: Query<'w, 's, (&'static mut ImageNode, Has<UiVisualLayer>)>,
    rectangles: Query<'w, 's, &'static mut UiSourceRect>,
    layouts: Query<'w, 's, &'static mut Node>,
}

impl DrawingWrites<'_, '_> {
    pub(super) fn apply(
        &mut self,
        context: PresentationContext,
        action: UiPresentationAction,
        missing: &mut bevy::ecs::entity::EntityHashMap<ImageNode>,
    ) {
        let Self {
            targets,
            roots,
            documents,
            commands,
            children,
            images,
            rectangles,
            layouts,
        } = self;
        let document = context.document;
        match action {
            UiPresentationAction::SetTargetNodeImageSourceRectangle {
                target_node,
                source_rectangle,
            } => {
                let Some(target) = targets.candidates(document, target_node).next() else {
                    return;
                };
                let next = UiSourceRect(source_rectangle);
                for entity in std::iter::once(target)
                    .chain(children.get(target).into_iter().flatten().copied())
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
            UiPresentationAction::SetTargetNodeImageOverride {
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
                for entity in std::iter::once(target)
                    .chain(children.get(target).into_iter().flatten().copied())
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
                    missing.insert(target, ImageNode::new(image));
                }
            }
            UiPresentationAction::SetTargetNodeLayoutPosition {
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
            UiPresentationAction::SetTargetNodeLayoutSize { target_node, size } => {
                if let Some(node) = targets.candidates(document, target_node).next() {
                    if let Ok(mut layout) = layouts.get_mut(node) {
                        layout.width = px(size[0] as f32);
                        layout.height = px(size[1] as f32);
                    }
                }
            }
            _ => unreachable!(),
        }
    }
}

#[derive(SystemParam)]
pub(super) struct TextWrites<'w, 's> {
    targets: AuthoredUiTargetLookup<'w, 's>,
    localizations: Res<'w, Assets<LocalizationAsset>>,
    precedence: Res<'w, LocalizationPrecedenceIndex>,
    texts: Query<'w, 's, &'static mut Text>,
}

impl TextWrites<'_, '_> {
    pub(super) fn apply(&mut self, context: PresentationContext, action: UiPresentationAction) {
        let Self {
            targets,
            localizations,
            precedence,
            texts,
        } = self;
        let candidates = |target| targets.candidates(context.document, target);
        let (target, value) = match &action {
            UiPresentationAction::CopySourceNodeTextToTargetNode {
                target_node,
                source_node,
            } => {
                let value = candidates(*source_node)
                    .find_map(|entity| texts.get(entity).ok())
                    .map(|text| text.0.clone());
                (*target_node, value)
            }
            UiPresentationAction::SetTargetNodeTextFromLocalizationKey {
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
            UiPresentationAction::SetTargetNodeTextLiteral { target_node, text } => {
                (*target_node, Some(text.clone()))
            }
            UiPresentationAction::FormatTargetNodeLocalizedInteger {
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
            _ => unreachable!(),
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
}

#[derive(SystemParam)]
pub(super) struct AudioWrites<'w, 's> {
    roots: Query<'w, 's, &'static UiDocumentRoot>,
    documents: Res<'w, Assets<UiDocumentAsset>>,
    sounds: MessageWriter<'w, PlayAudioClip>,
}

impl AudioWrites<'_, '_> {
    pub(super) fn apply(&mut self, context: PresentationContext, action: UiPresentationAction) {
        let Self {
            roots,
            documents,
            sounds,
        } = self;
        let UiPresentationAction::PlayDocumentAudioCue { audio_cue } = action else {
            return;
        };
        if let Some(clip) = roots
            .get(context.document)
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
}

#[derive(SystemParam)]
pub(super) struct CachedMessageWrites<'w, 's> {
    targets: AuthoredUiTargetLookup<'w, 's>,
    children: Query<'w, 's, &'static Children>,
    sequences: Query<'w, 's, &'static mut UiCachedMessages>,
    texts: Query<'w, 's, &'static mut Text>,
}

impl CachedMessageWrites<'_, '_> {
    pub(super) fn apply(&mut self, context: PresentationContext, action: UiPresentationAction) {
        let Self {
            targets,
            children,
            sequences,
            texts,
        } = self;
        let UiPresentationAction::AdvanceTargetNodeMessageCursor { target_node } = action else {
            return;
        };
        let Some(entity) = targets.candidates(context.document, target_node).next() else {
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
}

#[derive(SystemParam)]
pub(super) struct SelectionWrites<'w, 's> {
    targets: AuthoredUiTargetLookup<'w, 's>,
    documents: Res<'w, Assets<UiDocumentAsset>>,
    roots: Query<'w, 's, &'static UiDocumentRoot>,
    children: Query<'w, 's, &'static Children>,
    parents: Query<'w, 's, &'static ChildOf>,
    groups: Query<'w, 's, (), With<UiAuthoredToggleGroupSelectionPolicy>>,
    members: Query<'w, 's, &'static UiDocumentMembers>,
    owners: Query<'w, 's, &'static UiDocumentOwner>,
    selected: Query<'w, 's, (&'static mut UiSelected, Has<UiTypeListPolicy>)>,
    timelines: (
        Query<'w, 's, &'static mut UiAuthoredTimedActionSequencePlayback>,
        Query<'w, 's, (), With<UiShowHideAnimation>>,
        Query<'w, 's, &'static mut Visibility>,
    ),
    focusables: Query<'w, 's, &'static TabIndex, With<UiFocusable>>,
    focus_context: crate::active_authored_ui_context::ActiveAuthoredUiContext<'w, 's>,
    focus: ResMut<'w, InputFocus>,
    memory: Query<'w, 's, &'static mut UiFocusMemory>,
    operations: MessageWriter<'w, AuthoredUiDocumentOperation>,
    pending_animation: MessageWriter<'w, UiAnimationPlaybackRequest>,
    released_selections: MessageWriter<'w, UiSelectionWaitEnded>,
}

impl SelectionWrites<'_, '_> {
    pub(super) fn apply(
        &mut self,
        context: PresentationContext,
        action: UiPresentationAction,
        emitted: &mut Vec<UiNodeActivated>,
    ) {
        let Self {
            targets,
            documents,
            roots,
            children,
            parents,
            groups,
            members,
            owners,
            selected,
            timelines,
            focusables,
            focus_context,
            focus,
            memory,
            operations,
            pending_animation,
            released_selections,
        } = self;
        let (playback, animations, visibility) = timelines;
        let role_documents = || {
            children
                .get(context.lifecycle_owner)
                .into_iter()
                .flatten()
                .copied()
        };
        let mut emit = |node, trigger| {
            emitted.push(UiNodeActivated {
                node,
                source: context.source,
                trigger,
            });
        };
        match action {
            UiPresentationAction::SetTargetNodeSelectionAndTimedSequencesActive {
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
                        if let Some(selection) = timeline.restart(active, context.selection) {
                            released_selections.write(UiSelectionWaitEnded(selection));
                        }
                        if active {
                            if let Ok(children) = children.get(entity) {
                                for child in children.iter() {
                                    if animations.contains(child) {
                                        let _ = pending_animation.write_batch([
                                            UiAnimationPlaybackRequest {
                                                entity: child,
                                                operation: UiAnimationPlaybackOperation::Transition(
                                                    false,
                                                ),
                                            },
                                            UiAnimationPlaybackRequest {
                                                entity: child,
                                                operation: UiAnimationPlaybackOperation::Finish,
                                            },
                                        ]);
                                    }
                                    if let Ok(mut visibility) = visibility.get_mut(child) {
                                        if let Some(event) = set_authored_visibility(
                                            child,
                                            &mut visibility,
                                            Visibility::Hidden,
                                            owners.contains(child),
                                        ) {
                                            emitted.push(event);
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
            UiPresentationAction::SelectTargetNode { target_node } => {
                if let Some(node) = targets.candidates(context.document, target_node).next() {
                    if let Ok((mut selected, _)) = selected.get_mut(node) {
                        if selected.set_if_neq(UiSelected(true)) {
                            emit(node, UiTrigger::On);
                        }
                    }
                }
            }
            UiPresentationAction::ActivateSelectedChildOfTargetNode {
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
            UiPresentationAction::FocusPreviousOrNextSiblingOfTargetNode {
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
                        .filter(|entity| {
                            focusables.contains(*entity) && focus_context.enabled(*entity)
                        })
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
                    Some(policy.0),
                    if focus_next { 1 } else { -1 },
                    members.iter().filter_map(|entity| {
                        let candidate = focusables.get(entity).ok()?;
                        focus_context
                            .enabled(entity)
                            .then_some((candidate.0, entity))
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
            _ => unreachable!(),
        }
    }
}
