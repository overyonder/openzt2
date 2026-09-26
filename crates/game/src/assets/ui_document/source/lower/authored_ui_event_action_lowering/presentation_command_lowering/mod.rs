use crate::assets::source_document::path::AssetPath;
use crate::assets::source_document::ui::model::SourceUiEvent;
use crate::assets::ui_document::source::lower::authored_ui_action_argument_lowering::{
    cross_document_role, event_i32, role_target, target_node,
};
use crate::assets::ui_document::source::lower::authored_ui_asset_dependency_resolution::{
    cursor_texture_dependency, dependency, optional_resolved_audio_dependency_path,
    texture_dependency,
};
use crate::assets::ui_document::source::lower::authored_ui_document_lowering::AuthoredUiDocument;
use crate::assets::ui_document::source::lower::authored_ui_node_tree_lowering::BuildOutput;
use crate::assets::ui_document::source::lower::authored_ui_scalar_value_lowering::{
    invalid_at, parse_bool,
};
use crate::assets::ui_document::source::lower::canonical_source_value_resolution;
use openzt2_game_data::ui_document::action::information::{
    UiInformationAction, UiInformationActionRecord,
};
use openzt2_game_data::ui_document::action::presentation::{
    UiPresentationAction, UiPresentationActionRecord,
};
use openzt2_game_data::ui_document::action::{UiActionRecord, UiTrigger};
use openzt2_game_data::ui_document::document::UiDocumentRole;
use openzt2_game_data::AssetId;
use std::io;

pub(super) fn lower_presentation_command(
    trigger: UiTrigger,
    event: &SourceUiEvent,
    role: UiDocumentRole,
    current: AssetId,
    current_stable_name: &str,
    scroll_receiver: AssetId,
    output: &mut BuildOutput,
    input: &AuthoredUiDocument,
    scope: Option<&str>,
) -> io::Result<Option<UiActionRecord>> {
    let result = match event.message.as_str() {
        "UI_COPY_TEXT" if event.string.as_deref() == Some("zooname") => Ok(UiActionRecord::Information(UiInformationActionRecord {
            trigger,
            action: UiInformationAction::RenameZoo,
        })),
        "UI_CHILD" if event.child.is_none() => Ok(UiActionRecord::Presentation(UiPresentationActionRecord {
            trigger,
            action: UiPresentationAction::SetTargetNodeSelectionAndTimedSequencesActive {
                document_role: cross_document_role(event, input),
                target_node: target_node(event, cross_document_role(event, input).unwrap_or(role), current),
                active: true,
            },
        })),
        "UI_SHOW" | "UI_HIDE" if cross_document_role(event, input).is_some() => {
            let document_role = cross_document_role(event, input).expect("guarded above");
            Ok(UiActionRecord::Presentation(UiPresentationActionRecord {
                trigger,
                action: UiPresentationAction::SetDocumentNodeVisible {
                    document_role,
                    target_node: target_node(event, document_role, current),
                    visible: event.message == "UI_SHOW",
                },
            }))
        }
        // The settings panel's Back/Apply blocks broadcast UI_HIDE to their
        // containing panel. Explicit UI_CHILD receivers retain their target.
        "UI_HIDE" if role == UiDocumentRole::Options && event.target_child.is_none() && event.string.is_none() => Ok(UiActionRecord::Presentation(UiPresentationActionRecord {
            trigger,
            action: UiPresentationAction::HideOwningDocument,
        })),
        "ZT_ACTIVATE_MODE_HELP" => Ok(UiActionRecord::Presentation(UiPresentationActionRecord {
            trigger,
            action: UiPresentationAction::ShowDocumentRole {
                document_role: UiDocumentRole::ModeHelp,
            },
        })),
        "UI_COLLAPSE" => Ok(UiActionRecord::Presentation(UiPresentationActionRecord {
            trigger,
            action: UiPresentationAction::SetTargetNodeExpanded {
                target_node: target_node(event, role, current),
                expanded: false,
            },
        })),
        "UI_ALERT_RETURN" => Ok(UiActionRecord::Presentation(UiPresentationActionRecord {
            trigger,
            action: UiPresentationAction::HideOwningDocumentAfterAlertAcknowledgement,
        })),
        "ZT_GENERIC_CONFIRMATION_PROCEED" | "ZT_CANCEL_GENERIC_CONFIRMATION" => Ok(UiActionRecord::Presentation(UiPresentationActionRecord {
            trigger,
            action: UiPresentationAction::HideOwningDocumentAfterConfirmationDismissal,
        })),
        "UI_CHILD" => {
            return super::lower_authored_ui_event_to_canonical_action(
                trigger,
                event.child.as_deref().expect("guarded above"),
                role,
                current,
                current_stable_name,
                scroll_receiver,
                output,
                input,
                scope,
            )
        }
        "UI_SELF" => {
            return super::lower_authored_ui_event_to_canonical_action(
                trigger,
                event.child.as_deref().ok_or_else(|| invalid_at(input, "UI_SELF event has no typed child event"))?,
                role,
                current,
                current_stable_name,
                scroll_receiver,
                output,
                input,
                scope,
            )
        }
        "UI_SHOW" => Ok(UiActionRecord::Presentation(UiPresentationActionRecord {
            trigger,
            action: cross_document_role(event, input)
                .map(|document_role| UiPresentationAction::ShowDocumentRole { document_role })
                .unwrap_or_else(|| UiPresentationAction::SetTargetNodeVisible {
                    target_node: target_node(event, role, current),
                    visible: true,
                }),
        })),
        "UI_HIDE" => Ok(UiActionRecord::Presentation(UiPresentationActionRecord {
            trigger,
            action: cross_document_role(event, input)
                .map(|document_role| UiPresentationAction::HideDocumentRole { document_role })
                .unwrap_or_else(|| UiPresentationAction::SetTargetNodeVisible {
                    target_node: target_node(event, role, current),
                    visible: false,
                }),
        })),
        "UI_ACTIVATE" => Ok(UiActionRecord::Presentation(UiPresentationActionRecord {
            trigger,
            action: UiPresentationAction::ActivateTargetNodeWithPress {
                target_node: target_node(event, role, current),
            },
        })),
        "UI_ACTIVATE_ON" => Ok(UiActionRecord::Presentation(UiPresentationActionRecord {
            trigger,
            action: UiPresentationAction::SetTargetNodeSelectionAndTimedSequencesActive {
                document_role: cross_document_role(event, input),
                target_node: target_node(event, cross_document_role(event, input).unwrap_or(role), current),
                active: true,
            },
        })),
        "UI_ENABLE" => Ok(UiActionRecord::Presentation(UiPresentationActionRecord {
            trigger,
            action: UiPresentationAction::SetTargetNodeInteractionEnabled {
                target_node: target_node(event, role, current),
                enabled: true,
            },
        })),
        "UI_DISABLE" => Ok(UiActionRecord::Presentation(UiPresentationActionRecord {
            trigger,
            action: UiPresentationAction::SetTargetNodeInteractionEnabled {
                target_node: target_node(event, role, current),
                enabled: false,
            },
        })),
        "UI_ACTIVATE_OFF" => Ok(UiActionRecord::Presentation(UiPresentationActionRecord {
            trigger,
            action: UiPresentationAction::SetTargetNodeSelectionAndTimedSequencesActive {
                document_role: cross_document_role(event, input),
                target_node: target_node(event, cross_document_role(event, input).unwrap_or(role), current),
                active: false,
            },
        })),
        "UI_SCROLL" | "UI_SET_SCROLL" => Ok(UiActionRecord::Presentation(UiPresentationActionRecord {
            trigger,
            action: UiPresentationAction::ChangeTargetNodeSliderOrScrollPositionByDelta {
                target_node: if event.target_child.is_some() || event.string.is_some() {
                    target_node(event, role, current)
                } else {
                    scroll_receiver
                },
                position_delta: [event_i32(event, 0), event_i32(event, 1)],
            },
        })),
        "UI_SET_SRC_RECT" => Ok(UiActionRecord::Presentation(UiPresentationActionRecord {
            trigger,
            action: UiPresentationAction::SetTargetNodeImageSourceRectangle {
                target_node: target_node(event, role, current),
                source_rectangle: [event_i32(event, 0), event_i32(event, 1), event_i32(event, 2), event_i32(event, 3)],
            },
        })),
        "UI_SET_POS" => Ok(UiActionRecord::Presentation(UiPresentationActionRecord {
            trigger,
            action: UiPresentationAction::SetTargetNodeLayoutPosition {
                target_node: target_node(event, role, current),
                position: [event_i32(event, 0), event_i32(event, 1)],
            },
        })),
        "UI_COPY_TEXT" | "UI_COPY_NAME" => Ok(UiActionRecord::Presentation(UiPresentationActionRecord {
            trigger,
            action: UiPresentationAction::CopySourceNodeTextToTargetNode {
                target_node: event.target_child.as_deref().map(|target| UiDocumentRole::node_id(role, target)).unwrap_or(current),
                source_node: event.string.as_deref().map(|source| UiDocumentRole::node_id(role, source)).unwrap_or_default(),
            },
        })),
        "UI_SET_LOCID" => Ok(UiActionRecord::Presentation(UiPresentationActionRecord {
            trigger,
            action: UiPresentationAction::SetTargetNodeTextFromLocalizationKey {
                target_node: target_node(event, role, current),
                localization_key: canonical_source_value_resolution::lower_optional_authored_semantic_key_to_asset_id(event.string.as_deref()),
            },
        })),
        "UI_MOUSE_ENTER" => Ok(UiActionRecord::Presentation(UiPresentationActionRecord {
            trigger,
            action: UiPresentationAction::SetTargetNodeHoverPresentation {
                target_node: target_node(event, role, current),
                hover_presented: true,
            },
        })),
        "UI_EXPAND" => Ok(UiActionRecord::Presentation(UiPresentationActionRecord {
            trigger,
            action: UiPresentationAction::SetTargetNodeExpanded {
                target_node: target_node(event, role, current),
                expanded: true,
            },
        })),
        "UI_ACTIVATE_DATA" => Ok(UiActionRecord::Presentation(UiPresentationActionRecord {
            trigger,
            action: UiPresentationAction::SelectTargetNode {
                target_node: target_node(event, role, current),
            },
        })),
        "UI_WIND_ANIMATION" => Ok(UiActionRecord::Presentation(UiPresentationActionRecord {
            trigger,
            action: UiPresentationAction::WindTargetShowHideAnimationToCurrentDirectionBoundary {
                target_node: target_node(event, role, current),
            },
        })),
        "UI_NEXT_CACHED_MSG" => Ok(UiActionRecord::Presentation(UiPresentationActionRecord {
            trigger,
            action: UiPresentationAction::AdvanceTargetNodeMessageCursor {
                target_node: target_node(event, role, current),
            },
        })),
        "UI_CLOSE" => Ok(UiActionRecord::Presentation(UiPresentationActionRecord {
            trigger,
            action: UiPresentationAction::HideOwningDocument,
        })),
        "UI_SET_SIZE" => Ok(UiActionRecord::Presentation(UiPresentationActionRecord {
            trigger,
            action: UiPresentationAction::SetTargetNodeLayoutSize {
                target_node: target_node(event, role, current),
                size: [event_i32(event, 0), event_i32(event, 1)],
            },
        })),
        "UI_SET_IMAGE" => Ok(UiActionRecord::Presentation(UiPresentationActionRecord {
            trigger,
            action: UiPresentationAction::SetTargetNodeImageOverride {
                target_node: target_node(event, role, current),
                image_asset: event
                    .string
                    .as_deref()
                    .map(AssetPath::new)
                    .as_ref()
                    .map(|path| texture_dependency(Some(path.key()), output, input))
                    .transpose()?
                    .unwrap_or_default(),
            },
        })),
        "UI_SET_MODAL" => Ok(UiActionRecord::Presentation(UiPresentationActionRecord {
            trigger,
            action: UiPresentationAction::SetTargetNodeModal {
                target_node: target_node(event, role, current),
                modal: event.value.as_deref().and_then(parse_bool).unwrap_or(false),
            },
        })),
        "UI_SET_LTT" | "UI_SET_STT" => Ok(UiActionRecord::Presentation(UiPresentationActionRecord {
            trigger,
            action: UiPresentationAction::SetTargetNodeTooltip {
                target_node: target_node(event, role, current),
                localization_key: canonical_source_value_resolution::lower_optional_authored_semantic_key_to_asset_id(event.string.as_deref()),
                long_tooltip: event.message == "UI_SET_LTT",
            },
        })),
        "UI_SETCURSOR" => Ok(UiActionRecord::Presentation(UiPresentationActionRecord {
            trigger,
            action: UiPresentationAction::SetTargetNodeCursor {
                target_node: target_node(event, role, current),
                cursor_asset: cursor_texture_dependency(event.string.clone(), output, input)?,
            },
        })),
        "UI_SET_TEXT" => Ok(UiActionRecord::Presentation(UiPresentationActionRecord {
            trigger,
            action: UiPresentationAction::SetTargetNodeTextLiteral {
                target_node: target_node(event, role, current),
                text: (event.string.as_deref().unwrap_or_default()).to_owned(),
            },
        })),
        "UI_MOUSE_LEAVE" | "UI_LBUTTONUP" => Ok(UiActionRecord::Presentation(UiPresentationActionRecord {
            trigger,
            action: UiPresentationAction::SetTargetNodeHoverPresentation {
                target_node: target_node(event, role, current),
                hover_presented: false,
            },
        })),
        "UI_REPRESS" => Ok(UiActionRecord::Presentation(UiPresentationActionRecord {
            trigger,
            action: UiPresentationAction::ActivateSelectedChildOfTargetNode {
                target_node: role_target(event, input)
                    .or_else(|| {
                        (role == UiDocumentRole::InGameHud && event.target_child.as_deref().is_some_and(|target| target.eq_ignore_ascii_case("multi list tabs"))).then_some(UiDocumentRole::MultiList)
                    })
                    .filter(|target_role| *target_role != role)
                    .and_then(|target_role| event.target_child.as_deref().or(event.string.as_deref()).map(|target| UiDocumentRole::node_id(target_role, target)))
                    .unwrap_or_else(|| target_node(event, role, current)),
            },
        })),
        "UI_SET_NEXT" | "UI_SET_PREVIOUS" => Ok(UiActionRecord::Presentation(UiPresentationActionRecord {
            trigger,
            action: UiPresentationAction::FocusPreviousOrNextSiblingOfTargetNode {
                target_node: target_node(event, role, current),
                focus_next: event.message == "UI_SET_NEXT",
            },
        })),
        "UI_SHOW_CHILD_EX" | "UI_ROOT_CHILD" => Ok(UiActionRecord::Presentation(UiPresentationActionRecord {
            trigger,
            action: UiPresentationAction::ShowOnlyNamedChildOfTargetNode {
                target_node: target_node(event, role, current),
                child_node: event.string.as_deref().or(event.target_child.as_deref()).map(|child| UiDocumentRole::node_id(role, child)).unwrap_or_default(),
            },
        })),
        "UI_PLAY_SOUND" => Ok(UiActionRecord::Presentation(UiPresentationActionRecord {
            trigger,
            action: UiPresentationAction::PlayDocumentAudioCue {
                audio_cue: event
                    .string
                    .as_deref()
                    .map(|name| optional_resolved_audio_dependency_path(name, input).map(|path| path.map(|path| dependency(Some(path), output))))
                    .transpose()?
                    .flatten()
                    .unwrap_or_default(),
            },
        })),
        "UI_QUERY_TARGET" => Ok(UiActionRecord::Presentation(UiPresentationActionRecord {
            trigger,
            action: UiPresentationAction::MarkTargetNodeForQuery {
                target_node: target_node(event, role, current),
            },
        })),
        "UI_SET_VALUE" => Ok(UiActionRecord::Presentation(UiPresentationActionRecord {
            trigger,
            action: UiPresentationAction::SetTargetNodeIntegerValue {
                target_node: target_node(event, role, current),
                integer_value: event.value.as_deref().and_then(|value| value.parse::<f32>().ok()).map(|value| value.round() as i32).unwrap_or_default(),
            },
        })),
        _ => return Ok(None),
    };
    result.map(Some)
}
