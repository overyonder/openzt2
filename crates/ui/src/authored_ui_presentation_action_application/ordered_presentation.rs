use super::RequestUiPresentationAction;
use bevy::prelude::*;
use openzt2_game_assets::ui_document::ui_document_asset_types_and_borrowing_queries::UiDocumentAsset;
use openzt2_game_data::{
    ui_document::{action::presentation::UiPresentationAction, document::UiDocumentRole},
    AssetId,
};
use openzt2_interface_state::{
    input::input_types::ActionSource,
    ui::{
        authored_ui_action_projection_components::UiPresentationActions,
        authored_ui_activation_contracts::UiNodeActivated,
        authored_ui_node_projection_components::{UiDocumentOwner, UiDocumentRoot},
    },
};

mod receivers;

#[derive(Clone, Copy)]
struct PresentationContext {
    document: Entity,
    lifecycle_owner: Entity,
    source: ActionSource,
}
#[derive(Event)]
struct DocumentPresentation {
    context: PresentationContext,
    operation: DocumentOperation,
}
enum DocumentOperation {
    ShowDocumentRole {
        document_role: UiDocumentRole,
    },
    SetDocumentNodeVisible {
        document_role: UiDocumentRole,
        target_node: AssetId,
        visible: bool,
    },
    HideDocumentRole {
        document_role: UiDocumentRole,
    },
    HideOwningDocument,
}
#[derive(Event)]
struct VisibilityPresentation {
    context: PresentationContext,
    operation: VisibilityOperation,
}
enum VisibilityOperation {
    SetTargetNodeVisible {
        target_node: AssetId,
        visible: bool,
    },
    HideTargetNodeUnlessHovered {
        target_node: AssetId,
    },
    WindTargetShowHideAnimationToCurrentDirectionBoundary {
        target_node: AssetId,
    },
    ShowOnlyNamedChildOfTargetNode {
        target_node: AssetId,
        child_node: AssetId,
    },
    SetAllEmotePresentationsVisible {
        visible: bool,
    },
}
#[derive(Event)]
struct SelectionPresentation {
    context: PresentationContext,
    operation: SelectionOperation,
}
enum SelectionOperation {
    SetTargetNodeSelectionAndTimedSequencesActive {
        document_role: Option<UiDocumentRole>,
        target_node: AssetId,
        active: bool,
    },
    SelectTargetNode {
        target_node: AssetId,
    },
    ActivateSelectedChildOfTargetNode {
        target_node: AssetId,
        target_role: Option<UiDocumentRole>,
    },
    FocusPreviousOrNextSiblingOfTargetNode {
        target_node: AssetId,
        focus_next: bool,
    },
}
#[derive(Event)]
struct InteractionPresentation {
    context: PresentationContext,
    operation: InteractionOperation,
}
enum InteractionOperation {
    ActivateTargetNodeWithPress {
        target_node: AssetId,
    },
    SetTargetNodeInteractionEnabled {
        target_node: AssetId,
        enabled: bool,
    },
    SetTargetNodeModal {
        target_node: AssetId,
        modal: bool,
    },
    SetTargetNodeTooltip {
        target_node: AssetId,
        localization_key: AssetId,
        long_tooltip: bool,
    },
    SetTargetNodeCursor {
        target_node: AssetId,
        cursor_asset: AssetId,
    },
    SetTargetNodeHoverPresentation {
        target_node: AssetId,
        hover_presented: bool,
    },
    SetTargetNodeExpanded {
        target_node: AssetId,
        expanded: bool,
    },
}
#[derive(Event)]
struct ValuesPresentation {
    context: PresentationContext,
    operation: ValuesOperation,
}
enum ValuesOperation {
    SetTargetNodeIntegerValue {
        target_node: AssetId,
        integer_value: i32,
    },
    ChangeTargetNodeSliderOrScrollPositionByDelta {
        target_node: AssetId,
        position_delta: [i32; 2],
    },
}
#[derive(Event)]
struct DrawingPresentation {
    context: PresentationContext,
    operation: DrawingOperation,
}
enum DrawingOperation {
    SetTargetNodeImageSourceRectangle {
        target_node: AssetId,
        source_rectangle: [i32; 4],
    },
    SetTargetNodeImageOverride {
        target_node: AssetId,
        image_asset: AssetId,
    },
    SetTargetNodeLayoutPosition {
        target_node: AssetId,
        position: [i32; 2],
    },
    SetTargetNodeLayoutSize {
        target_node: AssetId,
        size: [i32; 2],
    },
}
#[derive(Event)]
struct TextPresentation {
    context: PresentationContext,
    operation: TextOperation,
}
enum TextOperation {
    CopySourceNodeTextToTargetNode {
        target_node: AssetId,
        source_node: AssetId,
    },
    FormatTargetNodeLocalizedInteger {
        target_node: AssetId,
        localization_key: AssetId,
        value: i64,
    },
    SetTargetNodeTextFromLocalizationKey {
        target_node: AssetId,
        localization_key: AssetId,
    },
    SetTargetNodeTextLiteral {
        target_node: AssetId,
        text: String,
    },
}
#[derive(Event)]
struct AudioPresentation {
    context: PresentationContext,
    operation: AudioOperation,
}
enum AudioOperation {
    PlayDocumentAudioCue { audio_cue: AssetId },
}
#[derive(Event)]
struct CachedMessagePresentation {
    context: PresentationContext,
    operation: CachedMessageOperation,
}
enum CachedMessageOperation {
    AdvanceTargetNodeMessageCursor { target_node: AssetId },
}

pub(crate) fn route_presentation_actions(
    mut commands: Commands,
    mut activations: MessageReader<UiNodeActivated>,
    mut requests: MessageReader<RequestUiPresentationAction>,
    documents: Res<Assets<UiDocumentAsset>>,
    sources: Query<(&UiDocumentOwner, &UiPresentationActions)>,
    roots: Query<(&UiDocumentRoot, &ChildOf)>,
) {
    for activation in activations.read() {
        let Ok((owner, actions)) = sources.get(activation.node) else {
            continue;
        };
        let Ok((root, parent)) = roots.get(owner.0) else {
            continue;
        };
        let Some(document) = documents.get(&root.document) else {
            continue;
        };
        let context = PresentationContext {
            document: owner.0,
            lifecycle_owner: parent.parent(),
            source: activation.source,
        };
        for record in actions.authored_action_records(document) {
            if record.trigger == activation.trigger {
                enqueue_presentation(&mut commands, context, &record.action);
            }
        }
    }
    for request in requests.read() {
        let Ok((_, parent)) = roots.get(request.document_root) else {
            continue;
        };
        enqueue_presentation(
            &mut commands,
            PresentationContext {
                document: request.document_root,
                lifecycle_owner: parent.parent(),
                source: ActionSource::System,
            },
            &request.action,
        );
    }
}

fn enqueue_presentation(
    commands: &mut Commands,
    context: PresentationContext,
    action: &UiPresentationAction,
) {
    match action {
        UiPresentationAction::ShowDocumentRole { document_role } => {
            commands.trigger(DocumentPresentation {
                context,
                operation: DocumentOperation::ShowDocumentRole {
                    document_role: *document_role,
                },
            })
        }
        UiPresentationAction::HideDocumentRole { document_role } => {
            commands.trigger(DocumentPresentation {
                context,
                operation: DocumentOperation::HideDocumentRole {
                    document_role: *document_role,
                },
            })
        }
        UiPresentationAction::SetDocumentNodeVisible {
            document_role,
            target_node,
            visible,
        } => commands.trigger(DocumentPresentation {
            context,
            operation: DocumentOperation::SetDocumentNodeVisible {
                document_role: *document_role,
                target_node: *target_node,
                visible: *visible,
            },
        }),
        UiPresentationAction::ActivateTargetNodeWithPress { target_node } => {
            commands.trigger(InteractionPresentation {
                context,
                operation: InteractionOperation::ActivateTargetNodeWithPress {
                    target_node: *target_node,
                },
            })
        }
        UiPresentationAction::SetTargetNodeVisible {
            target_node,
            visible,
        } => commands.trigger(VisibilityPresentation {
            context,
            operation: VisibilityOperation::SetTargetNodeVisible {
                target_node: *target_node,
                visible: *visible,
            },
        }),
        UiPresentationAction::HideTargetNodeUnlessHovered { target_node } => {
            commands.trigger(VisibilityPresentation {
                context,
                operation: VisibilityOperation::HideTargetNodeUnlessHovered {
                    target_node: *target_node,
                },
            })
        }
        UiPresentationAction::SetTargetNodeSelectionAndTimedSequencesActive {
            document_role,
            target_node,
            active,
        } => commands.trigger(SelectionPresentation {
            context,
            operation: SelectionOperation::SetTargetNodeSelectionAndTimedSequencesActive {
                document_role: *document_role,
                target_node: *target_node,
                active: *active,
            },
        }),
        UiPresentationAction::SetTargetNodeInteractionEnabled {
            target_node,
            enabled,
        } => commands.trigger(InteractionPresentation {
            context,
            operation: InteractionOperation::SetTargetNodeInteractionEnabled {
                target_node: *target_node,
                enabled: *enabled,
            },
        }),
        UiPresentationAction::SetTargetNodeIntegerValue {
            target_node,
            integer_value,
        } => commands.trigger(ValuesPresentation {
            context,
            operation: ValuesOperation::SetTargetNodeIntegerValue {
                target_node: *target_node,
                integer_value: *integer_value,
            },
        }),
        UiPresentationAction::ChangeTargetNodeSliderOrScrollPositionByDelta {
            target_node,
            position_delta,
        } => commands.trigger(ValuesPresentation {
            context,
            operation: ValuesOperation::ChangeTargetNodeSliderOrScrollPositionByDelta {
                target_node: *target_node,
                position_delta: *position_delta,
            },
        }),
        UiPresentationAction::SetTargetNodeImageSourceRectangle {
            target_node,
            source_rectangle,
        } => commands.trigger(DrawingPresentation {
            context,
            operation: DrawingOperation::SetTargetNodeImageSourceRectangle {
                target_node: *target_node,
                source_rectangle: *source_rectangle,
            },
        }),
        UiPresentationAction::SetTargetNodeLayoutPosition {
            target_node,
            position,
        } => commands.trigger(DrawingPresentation {
            context,
            operation: DrawingOperation::SetTargetNodeLayoutPosition {
                target_node: *target_node,
                position: *position,
            },
        }),
        UiPresentationAction::CopySourceNodeTextToTargetNode {
            target_node,
            source_node,
        } => commands.trigger(TextPresentation {
            context,
            operation: TextOperation::CopySourceNodeTextToTargetNode {
                target_node: *target_node,
                source_node: *source_node,
            },
        }),
        UiPresentationAction::FormatTargetNodeLocalizedInteger {
            target_node,
            localization_key,
            value,
        } => commands.trigger(TextPresentation {
            context,
            operation: TextOperation::FormatTargetNodeLocalizedInteger {
                target_node: *target_node,
                localization_key: *localization_key,
                value: *value,
            },
        }),
        UiPresentationAction::SetTargetNodeTextFromLocalizationKey {
            target_node,
            localization_key,
        } => commands.trigger(TextPresentation {
            context,
            operation: TextOperation::SetTargetNodeTextFromLocalizationKey {
                target_node: *target_node,
                localization_key: *localization_key,
            },
        }),
        UiPresentationAction::SetTargetNodeExpanded {
            target_node,
            expanded,
        } => commands.trigger(InteractionPresentation {
            context,
            operation: InteractionOperation::SetTargetNodeExpanded {
                target_node: *target_node,
                expanded: *expanded,
            },
        }),
        UiPresentationAction::SelectTargetNode { target_node } => {
            commands.trigger(SelectionPresentation {
                context,
                operation: SelectionOperation::SelectTargetNode {
                    target_node: *target_node,
                },
            })
        }
        UiPresentationAction::WindTargetShowHideAnimationToCurrentDirectionBoundary {
            target_node,
        } => commands.trigger(VisibilityPresentation {
            context,
            operation: VisibilityOperation::WindTargetShowHideAnimationToCurrentDirectionBoundary {
                target_node: *target_node,
            },
        }),
        UiPresentationAction::AdvanceTargetNodeMessageCursor { target_node } => {
            commands.trigger(CachedMessagePresentation {
                context,
                operation: CachedMessageOperation::AdvanceTargetNodeMessageCursor {
                    target_node: *target_node,
                },
            })
        }
        UiPresentationAction::SetTargetNodeLayoutSize { target_node, size } => {
            commands.trigger(DrawingPresentation {
                context,
                operation: DrawingOperation::SetTargetNodeLayoutSize {
                    target_node: *target_node,
                    size: *size,
                },
            })
        }
        UiPresentationAction::SetTargetNodeImageOverride {
            target_node,
            image_asset,
        } => commands.trigger(DrawingPresentation {
            context,
            operation: DrawingOperation::SetTargetNodeImageOverride {
                target_node: *target_node,
                image_asset: *image_asset,
            },
        }),
        UiPresentationAction::SetTargetNodeModal { target_node, modal } => {
            commands.trigger(InteractionPresentation {
                context,
                operation: InteractionOperation::SetTargetNodeModal {
                    target_node: *target_node,
                    modal: *modal,
                },
            })
        }
        UiPresentationAction::SetTargetNodeTooltip {
            target_node,
            localization_key,
            long_tooltip,
        } => commands.trigger(InteractionPresentation {
            context,
            operation: InteractionOperation::SetTargetNodeTooltip {
                target_node: *target_node,
                localization_key: *localization_key,
                long_tooltip: *long_tooltip,
            },
        }),
        UiPresentationAction::SetTargetNodeCursor {
            target_node,
            cursor_asset,
        } => commands.trigger(InteractionPresentation {
            context,
            operation: InteractionOperation::SetTargetNodeCursor {
                target_node: *target_node,
                cursor_asset: *cursor_asset,
            },
        }),
        UiPresentationAction::SetTargetNodeTextLiteral { target_node, text } => {
            commands.trigger(TextPresentation {
                context,
                operation: TextOperation::SetTargetNodeTextLiteral {
                    target_node: *target_node,
                    text: text.clone(),
                },
            })
        }
        UiPresentationAction::SetTargetNodeHoverPresentation {
            target_node,
            hover_presented,
        } => commands.trigger(InteractionPresentation {
            context,
            operation: InteractionOperation::SetTargetNodeHoverPresentation {
                target_node: *target_node,
                hover_presented: *hover_presented,
            },
        }),
        UiPresentationAction::ActivateSelectedChildOfTargetNode {
            target_node,
            target_role,
        } => commands.trigger(SelectionPresentation {
            context,
            operation: SelectionOperation::ActivateSelectedChildOfTargetNode {
                target_node: *target_node,
                target_role: *target_role,
            },
        }),
        UiPresentationAction::FocusPreviousOrNextSiblingOfTargetNode {
            target_node,
            focus_next,
        } => commands.trigger(SelectionPresentation {
            context,
            operation: SelectionOperation::FocusPreviousOrNextSiblingOfTargetNode {
                target_node: *target_node,
                focus_next: *focus_next,
            },
        }),
        UiPresentationAction::ShowOnlyNamedChildOfTargetNode {
            target_node,
            child_node,
        } => commands.trigger(VisibilityPresentation {
            context,
            operation: VisibilityOperation::ShowOnlyNamedChildOfTargetNode {
                target_node: *target_node,
                child_node: *child_node,
            },
        }),
        UiPresentationAction::PlayDocumentAudioCue { audio_cue } => {
            commands.trigger(AudioPresentation {
                context,
                operation: AudioOperation::PlayDocumentAudioCue {
                    audio_cue: *audio_cue,
                },
            })
        }
        UiPresentationAction::MarkTargetNodeForQuery { .. } => {}
        UiPresentationAction::HideOwningDocument
        | UiPresentationAction::HideOwningDocumentAfterAlertAcknowledgement
        | UiPresentationAction::HideOwningDocumentAfterConfirmationDismissal => {
            commands.trigger(DocumentPresentation {
                context,
                operation: DocumentOperation::HideOwningDocument,
            })
        }
        UiPresentationAction::SetAllEmotePresentationsVisible { visible } => {
            commands.trigger(VisibilityPresentation {
                context,
                operation: VisibilityOperation::SetAllEmotePresentationsVisible {
                    visible: *visible,
                },
            })
        }
    }
}

pub(crate) fn register_presentation_observers(app: &mut App) {
    app.add_observer(receivers::apply_document_presentation)
        .add_observer(receivers::apply_visibility_presentation)
        .add_observer(receivers::apply_selection_presentation)
        .add_observer(receivers::apply_interaction_presentation)
        .add_observer(receivers::apply_values_presentation)
        .add_observer(receivers::apply_drawing_presentation)
        .add_observer(receivers::apply_text_presentation)
        .add_observer(receivers::apply_audio_presentation)
        .add_observer(receivers::apply_cachedmessage_presentation);
}
