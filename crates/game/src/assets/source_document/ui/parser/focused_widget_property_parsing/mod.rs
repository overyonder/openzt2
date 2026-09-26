//! Focused authored widget presentation and property parsing.

use crate::assets::source_document::{
    ordered_source_document_types::{OrderedSourceDocumentAttribute, OrderedSourceDocumentNode},
    path::AssetPath,
};

use super::{
    super::model::{
        SourceUiAspect, SourceUiButton, SourceUiDragAxis, SourceUiField, SourceUiFinanceCategory,
        SourceUiGrid, SourceUiHelp, SourceUiImageEntry, SourceUiList, SourceUiListSource,
        SourceUiShowHideAnimation, SourceUiShowHideColors, SourceUiText,
    },
    source_scalar_and_attribute_reading::{
        copy_unknown_source_ui_attributes, optional_nonempty_source_attribute, parse_source_bool,
        parse_source_f32, parse_source_f32_attribute, parse_source_i32_attribute,
        source_attribute_value,
    },
    ui_aspect_and_visual_source_parsing::{
        parse_source_ui_color as parse_color,
        parse_source_ui_metric_rectangle as parse_metric_rect,
        parse_source_ui_rectangle as parse_rect,
    },
};

pub(super) fn parse_authored_show_hide_animation(
    node: &OrderedSourceDocumentNode,
) -> SourceUiShowHideAnimation {
    SourceUiShowHideAnimation {
        seconds: node.attribute("time").and_then(parse_source_f32),
        exit_rate: node.attribute("exitRate").and_then(parse_source_f32),
        initial_seconds: node.attribute("initialTime").and_then(parse_source_f32),
        initial_direction: node.attribute("initialDir").and_then(parse_source_bool),
        bob_seconds: node.attribute("bob").and_then(parse_source_f32),
        delay_seconds: node.attribute("delay").and_then(parse_source_f32),
        function: node.attribute("function").map(str::to_owned),
        start: node
            .element_children()
            .find(|node| node.name == "start")
            .and_then(parse_rect),
        end: node
            .element_children()
            .find(|node| node.name == "end")
            .and_then(parse_rect),
        colors: node
            .element_children()
            .find(|node| node.name == "colors")
            .map(parse_authored_show_hide_colors),
    }
}

pub(super) fn parse_authored_show_hide_colors(
    node: &OrderedSourceDocumentNode,
) -> SourceUiShowHideColors {
    SourceUiShowHideColors {
        affects_text: node.attribute("text").and_then(parse_source_bool),
        start: node
            .element_children()
            .find(|node| node.name == "start")
            .and_then(parse_color),
        end: node
            .element_children()
            .find(|node| node.name == "end")
            .and_then(parse_color),
    }
}

pub(super) fn parse_authored_ui_help(node: &OrderedSourceDocumentNode) -> SourceUiHelp {
    SourceUiHelp {
        ids: optional_nonempty_source_attribute(node, "ids"),
        name: optional_nonempty_source_attribute(node, "name"),
        short: optional_nonempty_source_attribute(node, "short"),
        long: optional_nonempty_source_attribute(node, "long"),
        help: optional_nonempty_source_attribute(node, "help"),
        lower: optional_nonempty_source_attribute(node, "lower"),
    }
}

pub(super) fn parse_authored_ui_field(node: &OrderedSourceDocumentNode) -> SourceUiField {
    SourceUiField {
        name: optional_nonempty_source_attribute(node, "name"),
        type_name: optional_nonempty_source_attribute(node, "type"),
        format: optional_nonempty_source_attribute(node, "fmt"),
        unknown_attributes: copy_unknown_source_ui_attributes(
            &node.attributes,
            &["name", "type", "fmt"],
        ),
    }
}

pub(super) fn parse_authored_finance_category(
    node: &OrderedSourceDocumentNode,
) -> Option<SourceUiFinanceCategory> {
    Some(SourceUiFinanceCategory {
        measurement: node.attribute("measurement")?.to_owned(),
        name: node.attribute("name")?.to_owned(),
        format: node
            .attribute("format")
            .filter(|value| !value.is_empty())
            .map(AssetPath::new),
        unknown_attributes: copy_unknown_source_ui_attributes(
            &node.attributes,
            &["measurement", "name", "format"],
        ),
    })
}

pub(super) fn parse_authored_ui_grid(
    attributes: &[OrderedSourceDocumentAttribute],
) -> SourceUiGrid {
    SourceUiGrid {
        auto_size: source_attribute_value(attributes, "autosize").and_then(parse_source_bool),
        auto_size_parent: source_attribute_value(attributes, "autosizeParent")
            .and_then(parse_source_bool),
        columns: parse_source_i32_attribute(attributes, "columns"),
        rows: parse_source_i32_attribute(attributes, "rows"),
        x_spacing: parse_source_i32_attribute(attributes, "xspacer"),
        y_spacing: parse_source_i32_attribute(attributes, "yspacer"),
        column_width: parse_source_i32_attribute(attributes, "columnWidth"),
        row_height: parse_source_i32_attribute(attributes, "rowHeight"),
        initial_x: parse_source_i32_attribute(attributes, "initialX"),
        initial_y: parse_source_i32_attribute(attributes, "initialY"),
    }
}

pub(super) fn parse_authored_ui_button(
    attributes: &[OrderedSourceDocumentAttribute],
) -> SourceUiButton {
    SourceUiButton {
        auto_size: source_attribute_value(attributes, "autosize").and_then(parse_source_bool),
        min_height: parse_source_i32_attribute(attributes, "minHeight"),
        toggle: source_attribute_value(attributes, "toggle").and_then(parse_source_bool),
        sticky: source_attribute_value(attributes, "sticky").and_then(parse_source_bool),
        repress: source_attribute_value(attributes, "repress").and_then(parse_source_bool),
        activate_data: source_attribute_value(attributes, "activateData").map(str::to_owned),
        repeat_delay_seconds: parse_source_f32_attribute(attributes, "repeatDelay")
            .or_else(|| parse_source_f32_attribute(attributes, "interval")),
        hold_change: parse_source_f32_attribute(attributes, "holdChange"),
        hold_interval_cap_seconds: parse_source_f32_attribute(attributes, "holdCap"),
        broadcast: source_attribute_value(attributes, "broadcast").and_then(parse_source_bool),
        delayed_activation_seconds: parse_source_f32_attribute(attributes, "delayTime"),
    }
}

pub(super) fn parse_authored_ui_list(
    attributes: &[OrderedSourceDocumentAttribute],
) -> SourceUiList {
    SourceUiList {
        grid: parse_authored_ui_grid(attributes),
        row_template: source_attribute_value(attributes, "openzt2RowTemplate").map(str::to_owned),
        balance_sheet_layout: source_attribute_value(attributes, "balanceSheetLayout")
            .map(str::to_owned),
        count_component: source_attribute_value(attributes, "countComponent").map(str::to_owned),
        update_seconds: parse_source_f32_attribute(attributes, "update"),
        source: match source_attribute_value(attributes, "openzt2ListSource") {
            Some("selectedEntityInventory") => SourceUiListSource::SelectedEntityInventory,
            Some("scenarioObjectives") => SourceUiListSource::ScenarioObjectives,
            Some("zoopediaTableOfContents") => SourceUiListSource::ZoopediaTableOfContents,
            _ => SourceUiListSource::Unbound,
        },
    }
}

pub(super) fn parse_authored_ui_drag_axis(value: Option<&str>) -> SourceUiDragAxis {
    match value.unwrap_or_default() {
        "x" => SourceUiDragAxis::X,
        "y" => SourceUiDragAxis::Y,
        "" => SourceUiDragAxis::Both,
        value => SourceUiDragAxis::Unknown(value.to_owned()),
    }
}

pub(super) fn parse_authored_ui_image_entry(
    node: &OrderedSourceDocumentNode,
) -> SourceUiImageEntry {
    SourceUiImageEntry {
        key: node.attribute("key").map(str::to_owned),
        localization_id: node.attribute("id").map(str::to_owned),
        image: node
            .attribute("image")
            .filter(|value| !value.is_empty())
            .map(AssetPath::new),
        rect: node
            .element_children()
            .find(|node| node.name == "BFRect")
            .map(parse_metric_rect),
        unknown_attributes: copy_unknown_source_ui_attributes(
            &node.attributes,
            &["key", "image", "id"],
        ),
    }
}

pub(super) fn parse_authored_ui_text(
    attributes: &[OrderedSourceDocumentAttribute],
    children: &[&OrderedSourceDocumentNode],
    aspect: Option<&SourceUiAspect>,
) -> SourceUiText {
    let text = children
        .iter()
        .copied()
        .find(|node| node.name == "text")
        .or_else(|| {
            children
                .iter()
                .copied()
                .find(|node| node.name == "UIAspect")
                .and_then(|aspect| aspect.element_children().find(|node| node.name == "text"))
        });
    SourceUiText {
        auto_size: source_attribute_value(attributes, "autosize")
            .and_then(parse_source_bool)
            .or_else(|| aspect.and_then(|aspect| aspect.auto_size)),
        min_height: parse_source_i32_attribute(attributes, "minHeight"),
        text_type: text
            .and_then(|node| optional_nonempty_source_attribute(node, "type"))
            .or_else(|| source_attribute_value(attributes, "type").map(str::to_owned))
            .filter(|value| !value.is_empty()),
        text_format: text.and_then(|node| optional_nonempty_source_attribute(node, "format")),
        localization_id: aspect.and_then(|aspect| aspect.localization_id.clone()),
        authored_string: aspect.and_then(|aspect| aspect.authored_string.clone()),
    }
}

pub(super) fn child_element_names(node: Option<&OrderedSourceDocumentNode>) -> Vec<String> {
    node.into_iter()
        .flat_map(|node| node.element_children())
        .map(|node| node.name.to_string())
        .collect()
}
