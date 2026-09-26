//! Authored UI region metrics, alignment, and initial state parsing.

use crate::assets::source_document::ordered_source_document_types::OrderedSourceDocumentNode;

use super::{
    super::model::{
        SourceUiActiveState, SourceUiAlignment, SourceUiMetric, SourceUiRegion, SourceUiState,
    },
    source_scalar_and_attribute_reading::copy_unknown_source_ui_attributes,
};

pub(super) fn parse_authored_ui_region(node: &OrderedSourceDocumentNode) -> SourceUiRegion {
    SourceUiRegion {
        x: SourceUiMetric::parse(node.attribute("x")),
        y: SourceUiMetric::parse(node.attribute("y")),
        width: SourceUiMetric::parse(node.attribute("w")),
        height: SourceUiMetric::parse(node.attribute("h")),
        x_align: SourceUiAlignment::parse(node.attribute("xalign"), SourceUiAlignment::Min),
        y_align: SourceUiAlignment::parse(node.attribute("yalign"), SourceUiAlignment::Min),
        width_align: SourceUiAlignment::parse(node.attribute("walign"), SourceUiAlignment::Length),
        height_align: SourceUiAlignment::parse(node.attribute("halign"), SourceUiAlignment::Length),
        unknown_attributes: copy_unknown_source_ui_attributes(
            &node.attributes,
            &["x", "y", "w", "h", "xalign", "yalign", "walign", "halign"],
        ),
    }
}

pub(super) fn parse_authored_ui_state(node: &OrderedSourceDocumentNode) -> SourceUiState {
    SourceUiState {
        active: match node.attribute("active") {
            Some("disable" | "disabled") => SourceUiActiveState::Disabled,
            Some("normal" | "enable" | "enabled") => SourceUiActiveState::Normal,
            Some(value) => SourceUiActiveState::Unknown(value.to_owned()),
            None => SourceUiActiveState::Normal,
        },
        visible: node.attribute("visible") != Some("hidden"),
    }
}
