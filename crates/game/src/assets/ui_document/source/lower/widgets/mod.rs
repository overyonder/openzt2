//! Blue Fang widget lowering into canonical UI widget records.

use crate::assets::source_document::ui::model::{
    SourceUiDragAxis, SourceUiNode, SourceUiWidgetData,
};
use crate::assets::ui_document::source::lower::authored_ui_asset_dependency_resolution::{
    resolved_asset_dependency, texture_dependency,
};
use crate::assets::ui_document::source::lower::authored_ui_document_lowering::{
    list_row_document_path, AuthoredUiDocument, ADOPTION_SLOT_ROW_DOCUMENT, CAMPAIGN_ROW_DOCUMENT,
    CAMPAIGN_SCENARIO_ROW_DOCUMENT, DISPLAY_RESOLUTION_ROW_DOCUMENT, FINANCE_CATEGORY_ROW_DOCUMENT, FINANCE_VALUE_ROW_DOCUMENT,
    PHOTO_ALBUM_CHOICE_ROW_DOCUMENT, PHOTO_CAMERA_ROLL_ROW_DOCUMENT, PROFILE_ROW_DOCUMENT,
};
use crate::assets::ui_document::source::lower::authored_ui_event_collection_lowering::authored_graph_type;
use crate::assets::ui_document::source::lower::authored_ui_node_tree_lowering::BuildOutput;
use crate::assets::ui_document::source::lower::authored_ui_scalar_value_lowering::{
    invalid_at, source_color,
};
use crate::assets::ui_document::source::lower::authored_ui_template_resolution::{
    lower_region, metric,
};
use crate::assets::ui_document::source::lower::authored_ui_widget_record_lowering::{
    axis, button_record, globe_record, grid_record, list_row_document, list_source, text_record,
    unbound_list_source,
};
use crate::assets::ui_document::source::lower::authored_ui_widget_vocabulary_resolution::{
    lower_authored_overview_map_layer_to_canonical_canvas,
    lower_authored_overview_map_layer_to_canonical_layer,
};
use crate::assets::ui_document::source::lower::canonical_source_value_resolution::{
    lower_authored_catalogue_key_to_asset_id, lower_optional_authored_semantic_key_to_asset_id,
};
use crate::assets::ui_document::source::lower::focused_timed_presentation_widget_lowering;
use openzt2_game_data::ui_document::document::UiDocumentRole;
use openzt2_game_data::ui_document::node_layout::UiNodeRegionDefinition;
use openzt2_game_data::ui_document::overview_map_presentation::{
    UiMapColorsRecord, UiMapLayerRecord, UiOverviewLayer,
};
use openzt2_game_data::ui_document::widget::{UiImageEntryRecord, UiWidgetRecord};
use openzt2_game_data::ui_document::widget_control::{
    UiDragOperation, UiNumericFormat, UiSliderRecord, UiTooltipPresentation,
};
use openzt2_game_data::ui_document::widget_live_collection::UiWidgetLiveCollectionSource;
use openzt2_game_data::AssetId;
use std::io;

pub(super) fn lower_source_ui_widget_to_canonical_widget_record(
    node: &SourceUiNode,
    index: u32,
    node_id_value: AssetId,
    output: &mut BuildOutput,
    input: &AuthoredUiDocument,
) -> io::Result<UiWidgetRecord> {
    let biome_panel = node
        .name
        .as_deref()
        .and_then(|name| name.strip_prefix("openzt2 biome panel:"))
        .map(|biome| UiWidgetRecord::BiomePanel {
            biome: lower_optional_authored_semantic_key_to_asset_id(Some(biome)),
        });
    if let Some(widget) = biome_panel {
        return Ok(widget);
    }
    if input.role == UiDocumentRole::PurchaseCatalogue
        && node
            .name
            .as_deref()
            .is_some_and(|name| name.eq_ignore_ascii_case("Buy Info Panel"))
    {
        let SourceUiWidgetData::Layout(grid) = &node.widget else {
            return Err(invalid_at(
                input,
                "normalized buy-information panel is not an authored grid",
            ));
        };
        return Ok(UiWidgetRecord::CatalogueDetails {
            grid: grid_record(grid),
        });
    }
    let widget = match &node.widget {
        SourceUiWidgetData::Plain => UiWidgetRecord::Plain,
        // No core archive authors a standalone UIAnimation widget. The parser
        // retains it for mod compatibility; its behavior lowers into the
        // node's canonical animation record rather than a parallel widget.
        SourceUiWidgetData::Animation(_) => UiWidgetRecord::Plain,
        SourceUiWidgetData::Layout(grid) => UiWidgetRecord::Grid(grid_record(grid)),
        SourceUiWidgetData::Button(button) => UiWidgetRecord::Button(button_record(button)),
        SourceUiWidgetData::CompositeButton { button, child_button, hover_child } => {
            let mut record = button_record(button);
            record.child_button = child_button.as_deref().map(|child| UiDocumentRole::node_id(input.role, child)).unwrap_or_default();
            record.hover_child = hover_child.as_deref().map(|child| UiDocumentRole::node_id(input.role, child)).unwrap_or_default();
            UiWidgetRecord::Button(record)
        }
        SourceUiWidgetData::List(list)
            if input.role == UiDocumentRole::ZooStatus && matches!(node.name.as_deref().map(str::to_ascii_lowercase).as_deref(), Some("econ totals categories" | "econ totals items")) =>
        {
            let labels = node.name.as_deref().is_some_and(|name| name.eq_ignore_ascii_case("econ totals categories"));
            UiWidgetRecord::FinanceList {
                grid: grid_record(&list.grid),
                row_document: AssetId::from_virtual_path(if labels { FINANCE_CATEGORY_ROW_DOCUMENT } else { FINANCE_VALUE_ROW_DOCUMENT }),
                labels,
            }
        }
        SourceUiWidgetData::List(list) => UiWidgetRecord::List {
            grid: grid_record(&list.grid),
            source: list_source(input.role, node.name.as_deref(), list),
            row_document: list_row_document(input.role, node.name.as_deref(), list),
            opener_node: AssetId::default(),
            drop_list_display_node: AssetId::default(),
            count_component: list.count_component.as_ref().map(|value| UiDocumentRole::node_id(input.role, value)).unwrap_or_default(),
            update_seconds: list.update_seconds.unwrap_or(0.0),
        },
        SourceUiWidgetData::DropList { list, opener, drop_list, .. } => UiWidgetRecord::List {
            grid: grid_record(&list.grid),
            source: list_source(input.role, node.name.as_deref(), list),
            row_document: list_row_document(input.role, node.name.as_deref(), list),
            opener_node: opener.as_ref().map(|value| UiDocumentRole::node_id(input.role, value)).unwrap_or_default(),
            drop_list_display_node: drop_list.as_ref().map(|value| UiDocumentRole::node_id(input.role, value)).unwrap_or_default(),
            count_component: list.count_component.as_ref().map(|value| UiDocumentRole::node_id(input.role, value)).unwrap_or_default(),
            update_seconds: list.update_seconds.unwrap_or(0.0),
        },
        SourceUiWidgetData::Slider(slider) => UiWidgetRecord::Slider(UiSliderRecord {
            minimum: slider.min.unwrap_or(0.0),
            maximum: slider.max.unwrap_or(1.0),
            increment: slider.increment.unwrap_or(1.0),
            initial: slider.initial_value.unwrap_or(0.0),
            // UISlider's constructor initializes minThumbSize to
            // eight pixels. Shipped scroll templates rely on that default
            // rather than redundantly authoring the attribute.
            minimum_thumb_size: slider.minimum_thumb_size.unwrap_or(8.0),
            axis: axis(slider.axis.as_ref().unwrap_or(&SourceUiDragAxis::Both), input)?,
            value_type: lower_optional_authored_semantic_key_to_asset_id(slider.value_type.as_deref()),
            span: slider.span.unwrap_or(0.0),
            thumb: slider.thumb_name.as_deref().map(|thumb| UiDocumentRole::node_id(input.role, thumb)).unwrap_or_default(),
            thumb_region: lower_region(slider.thumb_region.as_ref(), input)?,
            style: lower_optional_authored_semantic_key_to_asset_id(slider.style.as_deref()),
            field: lower_optional_authored_semantic_key_to_asset_id(slider.field.as_deref()),
            field_format: slider
                .field
                .as_deref()
                .and_then(|name| node.fields.iter().find(|field| field.name.as_deref() == Some(name)))
                .and_then(|field| field.format.as_deref())
                .map_or_else(UiNumericFormat::default, lower_numeric_format),
            on_change: lower_optional_authored_semantic_key_to_asset_id(slider.on_change.as_deref()),
        }),
        SourceUiWidgetData::Drag(drag) => UiWidgetRecord::Drag {
            minimum_width: drag.minimum_width.unwrap_or(0),
            maximum_width: drag.maximum_width.unwrap_or(0),
            minimum_height: drag.minimum_height.unwrap_or(0),
            maximum_height: drag.maximum_height.unwrap_or(0),
            bounded: drag.bounded.unwrap_or(false),
        },
        SourceUiWidgetData::DragCommand(command) => UiWidgetRecord::DragCommand {
            operation: match command.message.as_deref() {
                Some("UI_ADD_POS") => UiDragOperation::Move,
                Some("UI_ADD_SIZE") => UiDragOperation::Resize,
                Some("UI_MOUSE_DRAG") => UiDragOperation::Scroll,
                other => {
                    return Err(invalid_at(input, format!("unknown drag operation {other:?}")));
                }
            },
            axis: axis(&command.axis, input)?,
            flip_axis: command.flip.is_some(),
        },
        SourceUiWidgetData::Graph(graph) => UiWidgetRecord::Graph {
            graph_type: authored_graph_type(graph.graph_type.as_deref()),
            forced_minimum_y: graph.forced_minimum_y.unwrap_or(0.0),
            forced_maximum_y: graph.forced_maximum_y.unwrap_or(0.0),
            force_y_values: graph.force_y_values.unwrap_or(false),
            x_labels: graph.x_label_count.unwrap_or(10),
            y_labels: graph.y_label_count.unwrap_or(10),
        },
        SourceUiWidgetData::TypeList(list) => {
            let included_kinds = list.root_type.iter().chain(&list.root_types).map(|value| lower_authored_catalogue_key_to_asset_id(value)).collect();
            let excluded_kinds = list.excluded_root_types.iter().map(|value| lower_authored_catalogue_key_to_asset_id(value)).collect();
            let filters = list
                .filter_fields
                .as_deref()
                .unwrap_or_default()
                .iter()
                .map(|value| lower_authored_catalogue_key_to_asset_id(value))
                .collect();
            UiWidgetRecord::TypeList {
                grid: grid_record(&list.grid),
                included_kinds,
                excluded_kinds,
                row_document: AssetId::from_virtual_path(&list_row_document_path("purchaseicon")),
                filters,
            }
        }
        SourceUiWidgetData::WorldMap(world_map) => {
            let layers = &world_map.layers;
            let mut map_layers = Vec::new();
            for layer in layers {
                let kind = lower_authored_overview_map_layer_to_canonical_layer(layer, input)?;
                let icon = texture_dependency(layer.icon.as_ref().map(|value| value.key()), output, input)?;
                let marker_icon = match kind {
                    UiOverviewLayer::Animals => Some("ui/zoomap/animalcircle.dds"),
                    UiOverviewLayer::Buildings => Some("ui/zoomap/buildingsquare.dds"),
                    UiOverviewLayer::CameraPosition => Some("ui/zoomap/urhere.dds"),
                    _ => None,
                };
                let marker_icon = texture_dependency(marker_icon.map(str::to_owned), output, input)?;
                map_layers.push(UiMapLayerRecord {
                    kind,
                    node: layer.node.as_ref().map(|value| UiDocumentRole::node_id(input.role, value)).unwrap_or_default(),
                    icon,
                    marker_icon,
                    localization_key: layer.localization_id.as_ref().map(|value| AssetId::from_key(value)).unwrap_or_default(),
                    canvas: layer.has_canvas.then(|| lower_authored_overview_map_layer_to_canonical_canvas(layer, input)).transpose()?,
                });
            }
            UiWidgetRecord::WorldMap {
                layers: map_layers,
                colors: UiMapColorsRecord {
                    terrain: source_color(world_map.colors.terrain),
                    fence: source_color(world_map.colors.fence),
                    curb: source_color(world_map.colors.curb),
                    zoo_wall: source_color(world_map.colors.zoo_wall),
                    path: source_color(world_map.colors.path),
                    elevated_path: source_color(world_map.colors.elevated_path),
                    ground_track: source_color(world_map.colors.ground_track),
                    sky_track: source_color(world_map.colors.sky_track),
                    water: source_color(world_map.colors.water),
                },
            }
        }
        SourceUiWidgetData::MultiIcon(entries) => {
            let mut image_entries = Vec::new();
            for entry in entries {
                if !entry.unknown_attributes.is_empty() {
                    return Err(invalid_at(input, "unknown multi-icon attributes"));
                }
                let source_rect = if let Some(rect) = &entry.rect {
                    UiNodeRegionDefinition {
                        metrics: [metric(&rect.x, input)?, metric(&rect.y, input)?, metric(&rect.width, input)?, metric(&rect.height, input)?],
                        ..UiNodeRegionDefinition::default()
                    }
                } else {
                    UiNodeRegionDefinition::default()
                };
                let image = texture_dependency(entry.image.as_ref().map(|value| value.key()), output, input)?;
                image_entries.push(UiImageEntryRecord {
                    key: lower_optional_authored_semantic_key_to_asset_id(entry.key.as_deref()),
                    localization_key: lower_optional_authored_semantic_key_to_asset_id(entry.localization_id.as_deref()),
                    image,
                    source_rect,
                });
            }
            UiWidgetRecord::MultiIcon { entries: image_entries }
        }
        SourceUiWidgetData::Text(text) => {
            let mut lowered_text = text_record(text, output);
            // The native Zoopedia rich-content host clips its generated child
            // cells to the authored 640x100 viewport. Treating its `autosize`
            // flag as ordinary Bevy content sizing expands the background to
            // the whole page and obscures the surrounding controls.
            if input.role == UiDocumentRole::Zoopedia && node.name.as_deref().is_some_and(|name| name.eq_ignore_ascii_case("entrytextpage1")) {
                lowered_text.auto_size = false;
            }
            UiWidgetRecord::Text(lowered_text)
        }
        SourceUiWidgetData::TextEdit { text, edit } => UiWidgetRecord::TextEdit {
            text: text_record(text, output),
            maximum_length: edit.maximum_length.unwrap_or(0),
            legal_filename_only: edit.legal_filename_only.unwrap_or(false),
            highlight: edit.highlight.unwrap_or(false),
            change_sound: lower_optional_authored_semantic_key_to_asset_id(edit.change_sound.as_deref()),
            error_sound: lower_optional_authored_semantic_key_to_asset_id(edit.error_sound.as_deref()),
            cursor_on_seconds: edit.cursor_on_seconds.unwrap_or(0.5),
            cursor_off_seconds: edit.cursor_off_seconds.unwrap_or(0.5),
            changed_message: lower_optional_authored_semantic_key_to_asset_id(edit.changed_message.as_deref()),
        },
        SourceUiWidgetData::ToggleSet { grid, allow_repress, initial_column } => {
            let live_collection_source = unbound_list_source(input.role, node.name.as_deref());
            // These toggle sets are empty in their documents; the original game
            // fills them with rows loaded from these native row documents.
            let native_row_document = match live_collection_source {
                UiWidgetLiveCollectionSource::PhotoCameraRoll => Some(PHOTO_CAMERA_ROLL_ROW_DOCUMENT),
                UiWidgetLiveCollectionSource::PhotoAlbums => Some(PHOTO_ALBUM_CHOICE_ROW_DOCUMENT),
                UiWidgetLiveCollectionSource::Campaigns => Some(CAMPAIGN_ROW_DOCUMENT),
                UiWidgetLiveCollectionSource::CampaignScenarios => Some(CAMPAIGN_SCENARIO_ROW_DOCUMENT),
                _ => None,
            };
            if let Some(native_row_document) = native_row_document {
                UiWidgetRecord::List {
                    grid: grid_record(grid),
                    source: live_collection_source,
                    row_document: AssetId::from_virtual_path(native_row_document),
                    opener_node: AssetId::default(),
                    drop_list_display_node: AssetId::default(),
                    count_component: AssetId::default(),
                    update_seconds: 0.0,
                }
            } else if input.role == UiDocumentRole::Options && node.name.as_deref().is_some_and(|name| name.eq_ignore_ascii_case("ResolutionList")) {
                UiWidgetRecord::List {
                    grid: grid_record(grid),
                    source: UiWidgetLiveCollectionSource::DisplayResolutions,
                    row_document: AssetId::from_virtual_path(DISPLAY_RESOLUTION_ROW_DOCUMENT),
                    opener_node: AssetId::default(),
                    drop_list_display_node: AssetId::default(),
                    count_component: AssetId::default(),
                    update_seconds: 0.0,
                }
            } else if input.role == UiDocumentRole::ProfileSelect && node.name.as_deref().is_some_and(|name| name.eq_ignore_ascii_case("profilelist")) {
                UiWidgetRecord::List {
                    grid: grid_record(grid),
                    source: UiWidgetLiveCollectionSource::ProfileIndex,
                    row_document: AssetId::from_virtual_path(PROFILE_ROW_DOCUMENT),
                    opener_node: AssetId::default(),
                    drop_list_display_node: AssetId::default(),
                    count_component: AssetId::default(),
                    update_seconds: 0.0,
                }
            } else if node.name.as_deref() == Some("ZTAdoptionPanel") {
                UiWidgetRecord::AdoptionList {
                    grid: grid_record(grid),
                    row_document: AssetId::from_virtual_path(ADOPTION_SLOT_ROW_DOCUMENT),
                }
            } else {
                UiWidgetRecord::ToggleSet {
                    grid: grid_record(grid),
                    allow_repress: *allow_repress,
                    initial_column: *initial_column,
                    source: live_collection_source,
                }
            }
        }
        SourceUiWidgetData::Globe(globe) => {
            let record = globe_record(index, globe, output, input)?;
            UiWidgetRecord::Globe(record)
        }
        SourceUiWidgetData::RailCamera { scene } => {
            let path = scene.key();
            let id = AssetId::from_virtual_path(&path);
            output.dependencies.insert(id.0);
            output.explicit_scenes.insert(id.0, path);
            UiWidgetRecord::RailCamera { scene: id }
        }
        SourceUiWidgetData::Tooltip(tooltip) => UiWidgetRecord::Tooltip {
            text: text_record(&tooltip.text, output),
            presentation: match tooltip.tooltip_type.as_deref() {
                None | Some("name") => UiTooltipPresentation::Name,
                Some("short") => UiTooltipPresentation::Short,
                Some("long") => UiTooltipPresentation::Long,
                Some("help") => UiTooltipPresentation::Help,
                Some(_) => unreachable!("tooltip type was validated before compilation"),
            },
            floating: tooltip.floating.unwrap_or(false),
            autohide: tooltip.autohide.unwrap_or(false),
            offset: [tooltip.offset_x.unwrap_or_default(), tooltip.offset_y.unwrap_or_default()],
            appear_seconds: tooltip.appear_seconds.unwrap_or(0.0),
            display_seconds: tooltip.display_seconds.unwrap_or(0.0),
        },
        SourceUiWidgetData::TreeElement(tree) => UiWidgetRecord::TreeElement {
            grid: grid_record(&tree.grid),
            item: tree.item_id.as_ref().map(|value| AssetId::from_key(value)).unwrap_or_default(),
            expanded: tree.expanded.unwrap_or(false),
            selected: tree.selected.unwrap_or(false),
        },
        SourceUiWidgetData::Tool { command } => UiWidgetRecord::Tool {
            tool: lower_optional_authored_semantic_key_to_asset_id(command.as_deref()),
        },
        SourceUiWidgetData::TimedEvents(events) => focused_timed_presentation_widget_lowering::lower_source_ui_timed_events_to_canonical_focused_presentation_widget_record(
            node.name.as_deref(),
            node_id_value,
            events,
            input.role,
            output,
            input,
        )?,
        SourceUiWidgetData::XmlEdit { document } => UiWidgetRecord::XmlEdit {
            document: resolved_asset_dependency(document.as_ref().map(|value| value.key()), &input.resolved_dependencies.documents, "document", output, input)?,
        },
        SourceUiWidgetData::Window(window) => UiWidgetRecord::Window {
            modal: window.modal.unwrap_or(false),
            draggable: window.draggable.unwrap_or(false),
            horizontal: window.horizontal.unwrap_or(false),
            vertical: window.vertical.unwrap_or(false),
            wheel_scroll: window.wheel_scroll.unwrap_or(false),
            title: window.title.as_deref().map(|title| UiDocumentRole::node_id(input.role, title)).unwrap_or_default(),
            horizontal_scroll: window
                .horizontal_scroll
                .as_deref()
                .filter(|scroll| *scroll != "none")
                .map(|scroll| UiDocumentRole::node_id(input.role, scroll))
                .unwrap_or_default(),
            vertical_scroll: window
                .vertical_scroll
                .as_deref()
                .filter(|scroll| *scroll != "none")
                .map(|scroll| UiDocumentRole::node_id(input.role, scroll))
                .unwrap_or_default(),
        },
    };
    if matches!(&widget, UiWidgetRecord::TypeList { filters, .. } if !filters.is_empty()) {
        for path in [
            "ui/fragment/ui/layout/filterlistitem.xml",
            "ui/fragment/ui/layout/verticaldivide.xml",
        ] {
            output
                .dependencies
                .insert(AssetId::from_virtual_path(path).0);
        }
    }
    match &widget {
        UiWidgetRecord::List { row_document, .. }
        | UiWidgetRecord::TypeList { row_document, .. }
        | UiWidgetRecord::AdoptionList { row_document, .. }
        | UiWidgetRecord::FinanceList { row_document, .. } => {
            if *row_document != AssetId::default() {
                output.dependencies.insert(row_document.0);
            }
        }
        _ => {}
    }
    Ok(widget)
}

fn lower_numeric_format(format: &str) -> UiNumericFormat {
    let percent_suffix = format.ends_with("%%") || format.ends_with('%');
    let specifier = format.strip_prefix('%').unwrap_or(format);
    let specifier = specifier
        .strip_suffix("%%")
        .or_else(|| specifier.strip_suffix('%'))
        .unwrap_or(specifier);
    let (width, precision) = specifier
        .strip_suffix('f')
        .unwrap_or(specifier)
        .split_once('.')
        .unwrap_or(("", "0"));
    UiNumericFormat {
        minimum_width: width.parse().unwrap_or(0),
        decimal_places: precision.parse().unwrap_or(0),
        percent_suffix,
    }
}
