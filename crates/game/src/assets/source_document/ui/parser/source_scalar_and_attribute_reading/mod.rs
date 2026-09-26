//! Shared scalar and ordered source-attribute reading for UI parsing.

use crate::assets::source_document::ordered_source_document_types::{
    OrderedSourceDocumentAttribute, OrderedSourceDocumentNode,
};

use super::super::model::SourceUiAttribute;

pub(super) fn is_known_widget_attribute(name: &str) -> bool {
    const NAMES: &str = "name template templateName cursor cursorDirectory modal alwayshit canBeCachedWhenCalledFromLua useListBoxColorIfInContainer openzt2ListSource openzt2RowTemplate openzt2ScenarioObjectiveVisual openzt2RailCameraScene notifyTargetsOfCompletedAnimation xPack bgimg sx sy sw sh padx pady width exitRate initialTime initialDir idle frozen visible autosize autosizeParent minHeight toggle sticky repress activateData repeatDelay interval holdChange holdCap broadcast delayTime screamThreshold screamDelay button hover columns column rows xspacer yspacer columnWidth rowHeight initialX initialY balanceSheetLayout countComponent update droplistLayout horizontalLayout type span min max inc value thumb axis style field onChange minThumbSize minWidth maxWidth maxHeight bounded msg flip data string graphType graphTypeString numXLabels numYLabels forcedMinY forcedMaxY forceYAxisValues maxlength changesound errorsound legalFilenameOnly highlight timeon timeoff textchanged dotHighlightCursor selectionRotateSpeed mouseIncrement mouseDownFriction mouseUpFriction frictionTransitionTime moveTime command target float autohide offx offy appear display id expanded selected kind title draggable drag horizontal vertical wheelScroll hscroll vscroll path opener droplist";
    matches!(name, "The Droplist Layout" | "Horizontal Layout")
        || NAMES
            .split_ascii_whitespace()
            .any(|candidate| candidate == name)
}

pub(super) fn source_attribute_value<'a>(
    attributes: &'a [OrderedSourceDocumentAttribute],
    name: &str,
) -> Option<&'a str> {
    attributes
        .iter()
        .find(|attribute| attribute.name() == name)
        .map(OrderedSourceDocumentAttribute::value)
}

pub(super) fn optional_nonempty_source_attribute(
    node: &OrderedSourceDocumentNode,
    name: &str,
) -> Option<String> {
    node.attribute(name)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
}

pub(super) fn parse_source_bool(value: &str) -> Option<bool> {
    match value {
        "true" | "1" => Some(true),
        "false" | "0" => Some(false),
        _ => None,
    }
}

pub(super) fn parse_source_f32(value: &str) -> Option<f32> {
    value.trim_end_matches('f').parse().ok()
}

pub(super) fn parse_source_i32(value: &str) -> Option<i32> {
    value.parse().ok()
}

pub(super) fn parse_source_f32_attribute(
    attributes: &[OrderedSourceDocumentAttribute],
    name: &str,
) -> Option<f32> {
    source_attribute_value(attributes, name).and_then(parse_source_f32)
}

pub(super) fn parse_source_i32_attribute(
    attributes: &[OrderedSourceDocumentAttribute],
    name: &str,
) -> Option<i32> {
    source_attribute_value(attributes, name).and_then(parse_source_i32)
}

pub(super) fn copy_source_ui_attribute(
    attribute: &OrderedSourceDocumentAttribute,
) -> SourceUiAttribute {
    SourceUiAttribute {
        name: attribute.name().to_owned(),
        value: attribute.value().to_owned(),
    }
}

pub(super) fn copy_unknown_source_ui_attributes(
    attributes: &[OrderedSourceDocumentAttribute],
    known: &[&str],
) -> Vec<SourceUiAttribute> {
    attributes
        .iter()
        .filter(|attribute| !known.contains(&attribute.name()))
        .map(copy_source_ui_attribute)
        .collect()
}
