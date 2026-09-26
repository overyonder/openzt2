//! Dispatch from one authored UI widget kind to its transient typed payload.

use crate::assets::source_document::{
    ordered_source_document_types::{OrderedSourceDocumentAttribute, OrderedSourceDocumentNode},
    path::AssetPath,
};

use super::{
    super::model::{
        SourceUiAnimation, SourceUiAspect, SourceUiDrag, SourceUiDragCommand, SourceUiGraph,
        SourceUiMapColors, SourceUiMapLayer, SourceUiSlider, SourceUiTextEdit, SourceUiTimedEvent,
        SourceUiTooltip, SourceUiTreeElement, SourceUiTypeList, SourceUiWidgetData,
        SourceUiWidgetKind, SourceUiWindow, SourceUiWorldMap,
    },
    focused_widget_property_parsing::{
        child_element_names as child_names, parse_authored_show_hide_colors as parse_colors,
        parse_authored_ui_button as parse_button, parse_authored_ui_drag_axis as parse_drag_axis,
        parse_authored_ui_grid as parse_grid, parse_authored_ui_image_entry as parse_image_entry,
        parse_authored_ui_list as parse_list, parse_authored_ui_text as parse_text,
    },
    globe_source_extraction::extract_authored_ui_globe_source as parse_globe,
    source_scalar_and_attribute_reading::{
        optional_nonempty_source_attribute as optional_attr, parse_source_bool as parse_bool,
        parse_source_f32 as parse_f32, parse_source_f32_attribute as parse_f32_attr,
        parse_source_i32_attribute as parse_i32_attr, source_attribute_value as attr,
    },
    ui_aspect_and_visual_source_parsing::{
        parse_source_ui_color as parse_color, parse_source_ui_rectangle as parse_rect,
    },
    ui_event_and_hotkey_parsing::parse_authored_ui_event as parse_event,
    ui_region_and_state_parsing::parse_authored_ui_region as parse_region,
    SourceUiDiagnostic,
};

pub(super) fn parse_authored_ui_widget_data(
    kind: &SourceUiWidgetKind,
    attributes: &[OrderedSourceDocumentAttribute],
    children: &[&OrderedSourceDocumentNode],
    aspect: Option<&SourceUiAspect>,
    diagnostics: &mut Vec<SourceUiDiagnostic>,
) -> SourceUiWidgetData {
    let child = |name: &str| children.iter().copied().find(|node| node.name == name);
    match kind {
        SourceUiWidgetKind::Animation => SourceUiWidgetData::Animation(SourceUiAnimation {
            seconds: parse_f32_attr(attributes, "time"),
            start: child("start").and_then(parse_rect),
            end: child("end").and_then(parse_rect),
            colors: child("colors").map(parse_colors),
            exit_rate: parse_f32_attr(attributes, "exitRate"),
            initial_time: parse_f32_attr(attributes, "initialTime"),
            initial_direction: attr(attributes, "initialDir").and_then(parse_bool),
            bob_seconds: parse_f32_attr(attributes, "bob"),
            delay_seconds: parse_f32_attr(attributes, "delay"),
            function: attr(attributes, "function").map(str::to_owned),
        }),
        SourceUiWidgetKind::Button
        | SourceUiWidgetKind::HoverButton
        | SourceUiWidgetKind::ToggleButton
        | SourceUiWidgetKind::ToggleHoverButton
        | SourceUiWidgetKind::FullscreenButton
        | SourceUiWidgetKind::PushButton => SourceUiWidgetData::Button(parse_button(attributes)),
        SourceUiWidgetKind::CompositeButton | SourceUiWidgetKind::CompositeHoverButton => {
            SourceUiWidgetData::CompositeButton {
                button: parse_button(attributes),
                child_button: attr(attributes, "button").map(str::to_owned),
                hover_child: attr(attributes, "hover").map(str::to_owned),
            }
        }
        SourceUiWidgetKind::Container
        | SourceUiWidgetKind::Layout
        | SourceUiWidgetKind::PhotoAlbum => SourceUiWidgetData::Layout(parse_grid(attributes)),
        SourceUiWidgetKind::RailCamera => SourceUiWidgetData::RailCamera {
            scene: AssetPath::new(attr(attributes, "openzt2RailCameraScene").unwrap_or_default()),
        },
        SourceUiWidgetKind::ListBox
        | SourceUiWidgetKind::MultiList
        | SourceUiWidgetKind::GoalPanel => SourceUiWidgetData::List(parse_list(attributes)),
        SourceUiWidgetKind::DropList => SourceUiWidgetData::DropList {
            list: parse_list(attributes),
            layout: attr(attributes, "droplistLayout")
                .or_else(|| attr(attributes, "The Droplist Layout"))
                .map(str::to_owned),
            horizontal_layout: attr(attributes, "horizontalLayout")
                .or_else(|| attr(attributes, "Horizontal Layout"))
                .map(str::to_owned),
            opener: attr(attributes, "opener").map(str::to_owned),
            drop_list: attr(attributes, "droplist").map(str::to_owned),
        },
        SourceUiWidgetKind::Slider => {
            let thumb = child("thumb");
            SourceUiWidgetData::Slider(SourceUiSlider {
                value_type: attr(attributes, "type").map(str::to_owned),
                span: parse_f32_attr(attributes, "span"),
                min: parse_f32_attr(attributes, "min"),
                max: parse_f32_attr(attributes, "max"),
                increment: parse_f32_attr(attributes, "inc"),
                initial_value: parse_f32_attr(attributes, "value"),
                thumb_name: attr(attributes, "thumb")
                    .or_else(|| thumb.and_then(|node| node.attribute("name")))
                    .map(str::to_owned),
                thumb_region: thumb
                    .and_then(|node| node.element_children().find(|node| node.name == "UIRegion"))
                    .map(parse_region),
                axis: attr(attributes, "axis")
                    .or_else(|| thumb.and_then(|node| node.attribute("axis")))
                    .map(|axis| parse_drag_axis(Some(axis))),
                style: attr(attributes, "style")
                    .or_else(|| thumb.and_then(|node| node.attribute("style")))
                    .map(str::to_owned),
                field: attr(attributes, "field")
                    .or_else(|| child("field").and_then(|node| node.attribute("name")))
                    .map(str::to_owned),
                on_change: attr(attributes, "onChange").map(str::to_owned),
                minimum_thumb_size: parse_f32_attr(attributes, "minThumbSize"),
            })
        }
        SourceUiWidgetKind::Drag => SourceUiWidgetData::Drag(SourceUiDrag {
            minimum_width: parse_i32_attr(attributes, "minWidth"),
            maximum_width: parse_i32_attr(attributes, "maxWidth"),
            minimum_height: parse_i32_attr(attributes, "minHeight"),
            maximum_height: parse_i32_attr(attributes, "maxHeight"),
            bounded: attr(attributes, "bounded").and_then(parse_bool),
        }),
        SourceUiWidgetKind::DragCommand => SourceUiWidgetData::DragCommand(SourceUiDragCommand {
            message: attr(attributes, "msg").map(str::to_owned),
            axis: parse_drag_axis(attr(attributes, "axis")),
            flip: attr(attributes, "flip")
                .filter(|value| !value.is_empty())
                .map(str::to_owned),
            data: attr(attributes, "data").map(str::to_owned),
            string: attr(attributes, "string").map(str::to_owned),
        }),
        SourceUiWidgetKind::Graph => SourceUiWidgetData::Graph(SourceUiGraph {
            minimum: parse_f32_attr(attributes, "min"),
            maximum: parse_f32_attr(attributes, "max"),
            graph_type: attr(attributes, "graphType")
                .or_else(|| attr(attributes, "graphTypeString"))
                .map(str::to_owned),
            x_label_count: parse_i32_attr(attributes, "numXLabels"),
            y_label_count: parse_i32_attr(attributes, "numYLabels"),
            forced_minimum_y: parse_f32_attr(attributes, "forcedMinY"),
            forced_maximum_y: parse_f32_attr(attributes, "forcedMaxY"),
            force_y_values: attr(attributes, "forceYAxisValues").and_then(parse_bool),
        }),
        SourceUiWidgetKind::MultiIcon => SourceUiWidgetData::MultiIcon(
            child("images")
                .into_iter()
                .flat_map(|images| images.element_children())
                .filter(|node| node.name == "entry")
                .map(parse_image_entry)
                .collect(),
        ),
        SourceUiWidgetKind::WorldMap => {
            let layers = child("layers")
                .into_iter()
                .flat_map(OrderedSourceDocumentNode::element_children)
                .map(|layer| SourceUiMapLayer {
                    kind: layer.name.to_string(),
                    node: optional_attr(layer, "ui"),
                    icon: layer.attribute("icon").map(AssetPath::new),
                    localization_id: optional_attr(layer, "name"),
                    source: optional_attr(layer, "ref"),
                    has_canvas: layer
                        .attribute("hasCanvas")
                        .and_then(parse_bool)
                        .unwrap_or(true),
                })
                .collect();
            let mut colors = SourceUiMapColors::default();
            if let Some(authored) = child("colors") {
                for color in authored.element_children() {
                    let Some(value) = parse_color(color) else {
                        diagnostics.push(SourceUiDiagnostic::InvalidValue {
                            widget: "UIWorldMap".to_owned(),
                            field: color.name.to_string(),
                            value: color
                                .attributes
                                .iter()
                                .map(|attribute| {
                                    format!("{}={}", attribute.name(), attribute.value())
                                })
                                .collect::<Vec<_>>()
                                .join(" "),
                            span: color.span,
                        });
                        continue;
                    };
                    match color.name.as_str() {
                        "terrain" => colors.terrain = value,
                        "fence" => colors.fence = value,
                        "curb" => colors.curb = value,
                        "zoowall" => colors.zoo_wall = value,
                        "path" => colors.path = value,
                        "elevatedpath" => colors.elevated_path = value,
                        "groundtrack" => colors.ground_track = value,
                        "skytrack" => colors.sky_track = value,
                        "water" => colors.water = value,
                        _ => diagnostics.push(SourceUiDiagnostic::UnknownElement {
                            widget: "UIWorldMap".to_owned(),
                            element: color.name.to_string(),
                            span: color.span,
                        }),
                    }
                }
            }
            SourceUiWidgetData::WorldMap(SourceUiWorldMap { layers, colors })
        }
        SourceUiWidgetKind::Text | SourceUiWidgetKind::TextBuffer | SourceUiWidgetKind::Static => {
            SourceUiWidgetData::Text(parse_text(attributes, children, aspect))
        }
        SourceUiWidgetKind::TextEdit => SourceUiWidgetData::TextEdit {
            text: parse_text(attributes, children, aspect),
            edit: SourceUiTextEdit {
                maximum_length: parse_i32_attr(attributes, "maxlength"),
                change_sound: attr(attributes, "changesound").map(str::to_owned),
                error_sound: attr(attributes, "errorsound").map(str::to_owned),
                legal_filename_only: attr(attributes, "legalFilenameOnly").and_then(parse_bool),
                highlight: attr(attributes, "highlight").and_then(parse_bool),
                cursor_on_seconds: parse_f32_attr(attributes, "timeon"),
                cursor_off_seconds: parse_f32_attr(attributes, "timeoff"),
                changed_message: attr(attributes, "textchanged").map(str::to_owned),
            },
        },
        SourceUiWidgetKind::ToggleSet => SourceUiWidgetData::ToggleSet {
            grid: parse_grid(attributes),
            allow_repress: attr(attributes, "repress")
                .and_then(parse_bool)
                .unwrap_or(false),
            initial_column: parse_i32_attr(attributes, "column").unwrap_or(-1),
        },
        SourceUiWidgetKind::Globe => SourceUiWidgetData::Globe(parse_globe(attributes, children)),
        SourceUiWidgetKind::Tool => SourceUiWidgetData::Tool {
            command: attr(attributes, "command").map(str::to_owned),
        },
        SourceUiWidgetKind::Tooltip => SourceUiWidgetData::Tooltip(SourceUiTooltip {
            // `UITooltip` derives from `UIText` in the native UI. Its text
            // payload controls formatting and sizing even though the displayed
            // value is selected from the currently hovered node.
            text: {
                let mut text = parse_text(attributes, children, aspect);
                // The widget's `type` attribute selects name/short/long/help;
                // it is not the inherited UIText single/multi selector.
                text.text_type = children
                    .iter()
                    .copied()
                    .find(|node| node.name == "text")
                    .and_then(|node| optional_attr(node, "type"));
                text
            },
            target: attr(attributes, "target").map(str::to_owned),
            tooltip_type: attr(attributes, "type").map(str::to_owned),
            floating: attr(attributes, "float").and_then(parse_bool),
            autohide: attr(attributes, "autohide").and_then(parse_bool),
            offset_x: parse_i32_attr(attributes, "offx"),
            offset_y: parse_i32_attr(attributes, "offy"),
            appear_seconds: parse_f32_attr(attributes, "appear"),
            display_seconds: parse_f32_attr(attributes, "display"),
        }),
        SourceUiWidgetKind::TreeElement => SourceUiWidgetData::TreeElement(SourceUiTreeElement {
            grid: parse_grid(attributes),
            item_id: attr(attributes, "id").map(str::to_owned),
            expanded: attr(attributes, "expanded").and_then(parse_bool),
            selected: attr(attributes, "selected").and_then(parse_bool),
        }),
        SourceUiWidgetKind::TimedEvents => SourceUiWidgetData::TimedEvents(
            child("timedEvents")
                .into_iter()
                .flat_map(|node| node.element_children())
                .filter(|node| node.name == "event")
                .map(|node| SourceUiTimedEvent {
                    delay_seconds: node.attribute("time").and_then(parse_f32).unwrap_or(0.0),
                    event: parse_event(node),
                })
                .collect(),
        ),
        SourceUiWidgetKind::TypeList | SourceUiWidgetKind::ContextList => {
            SourceUiWidgetData::TypeList(SourceUiTypeList {
                grid: parse_grid(attributes),
                root_type: attr(attributes, "kind")
                    .filter(|value| !value.is_empty())
                    .map(str::to_owned),
                root_types: child_names(child("kinds")),
                excluded_root_types: child_names(child("kindsExclude")),
                non_remembered_root_types: child_names(child("nonRememberedKinds")),
                filter_fields: child("TypeListFilters").map(|node| child_names(Some(node))),
            })
        }
        SourceUiWidgetKind::Window => SourceUiWidgetData::Window(SourceUiWindow {
            title: attr(attributes, "title").map(str::to_owned),
            modal: attr(attributes, "modal").and_then(parse_bool),
            draggable: attr(attributes, "draggable")
                .or_else(|| attr(attributes, "drag"))
                .and_then(parse_bool),
            horizontal: attr(attributes, "horizontal").and_then(parse_bool),
            vertical: attr(attributes, "vertical").and_then(parse_bool),
            wheel_scroll: attr(attributes, "wheelScroll").and_then(parse_bool),
            horizontal_scroll: attr(attributes, "hscroll")
                .or_else(|| child("hscroll").and_then(|node| node.attribute("name")))
                .map(str::to_owned),
            vertical_scroll: attr(attributes, "vscroll")
                .or_else(|| child("vscroll").and_then(|node| node.attribute("name")))
                .map(str::to_owned),
        }),
        SourceUiWidgetKind::XmlEdit => SourceUiWidgetData::XmlEdit {
            document: attr(attributes, "path").map(AssetPath::new),
        },
        _ => SourceUiWidgetData::Plain,
    }
}
