//! UI presentation, focus, visibility, text, image, audio, and interaction actions.

use super::super::document::UiDocumentRole;
use super::UiTrigger;
use crate::AssetId;
use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize, Clone, Debug, PartialEq, Eq)]
pub struct UiPresentationActionRecord {
    pub trigger: UiTrigger,
    pub action: UiPresentationAction,
}

#[derive(Deserialize, Serialize, Clone, Debug, PartialEq, Eq)]
pub enum UiPresentationAction {
    ShowDocumentRole {
        document_role: UiDocumentRole,
    },
    HideDocumentRole {
        document_role: UiDocumentRole,
    },
    /// Preserve the authored child target when it belongs to another document.
    SetDocumentNodeVisible {
        document_role: UiDocumentRole,
        target_node: AssetId,
        visible: bool,
    },
    ActivateTargetNodeWithPress {
        target_node: AssetId,
    },
    SetTargetNodeVisible {
        target_node: AssetId,
        visible: bool,
    },
    SetTargetNodeSelectionAndTimedSequencesActive {
        /// A foreign target remains under the same UI lifecycle owner.
        #[serde(default)]
        document_role: Option<UiDocumentRole>,
        target_node: AssetId,
        active: bool,
    },
    SetTargetNodeInteractionEnabled {
        target_node: AssetId,
        enabled: bool,
    },
    SetTargetNodeIntegerValue {
        target_node: AssetId,
        integer_value: i32,
    },
    ChangeTargetNodeSliderOrScrollPositionByDelta {
        target_node: AssetId,
        position_delta: [i32; 2],
    },
    SetTargetNodeImageSourceRectangle {
        target_node: AssetId,
        source_rectangle: [i32; 4],
    },
    SetTargetNodeLayoutPosition {
        target_node: AssetId,
        position: [i32; 2],
    },
    CopySourceNodeTextToTargetNode {
        target_node: AssetId,
        source_node: AssetId,
    },
    SetTargetNodeTextFromLocalizationKey {
        target_node: AssetId,
        localization_key: AssetId,
    },
    SetTargetNodeExpanded {
        target_node: AssetId,
        expanded: bool,
    },
    SelectTargetNode {
        target_node: AssetId,
    },
    /// Drive the node's current show/hide animation to the boundary selected
    /// by its existing direction without changing that direction.
    WindTargetShowHideAnimationToCurrentDirectionBoundary {
        target_node: AssetId,
    },
    AdvanceTargetNodeMessageCursor {
        target_node: AssetId,
    },
    SetTargetNodeLayoutSize {
        target_node: AssetId,
        size: [i32; 2],
    },
    SetTargetNodeImageOverride {
        target_node: AssetId,
        image_asset: AssetId,
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
    SetTargetNodeTextLiteral {
        target_node: AssetId,
        text: String,
    },
    SetTargetNodeHoverPresentation {
        target_node: AssetId,
        hover_presented: bool,
    },
    ActivateSelectedChildOfTargetNode {
        target_node: AssetId,
    },
    FocusPreviousOrNextSiblingOfTargetNode {
        target_node: AssetId,
        focus_next: bool,
    },
    ShowOnlyNamedChildOfTargetNode {
        target_node: AssetId,
        child_node: AssetId,
    },
    PlayDocumentAudioCue {
        audio_cue: AssetId,
    },
    MarkTargetNodeForQuery {
        target_node: AssetId,
    },
    HideOwningDocument,
    HideOwningDocumentAfterAlertAcknowledgement,
    HideOwningDocumentAfterConfirmationDismissal,
    SetAllEmotePresentationsVisible {
        visible: bool,
    },
}
