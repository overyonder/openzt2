use super::authored_node_layout_projection::apply_authored_node_region_to_bevy_node;
use super::authored_text_projection::apply_authored_text_record_to_bevy_text_and_layout;
use bevy::prelude::*;
use openzt2_game_data::ui_document::action::scenarios::ObjectiveStatusFilter;
use openzt2_game_data::ui_document::document::UiDocumentRole;
use openzt2_game_data::ui_document::node::UiNodeDefinition;
use openzt2_game_data::ui_document::node::UiNodeKind;
use openzt2_game_data::ui_document::node_property_binding::UiNodePropertyBinding;
use openzt2_game_data::ui_document::node_style::UiStyleFlags;
use openzt2_game_data::ui_document::widget::UiWidgetRecord;
use openzt2_game_data::ui_document::widget_control::UiAxis;
use openzt2_game_data::ui_document::widget_control::UiTooltipPresentation;
use openzt2_game_data::ui_document::widget_live_collection::UiWidgetLiveCollectionSource;
use openzt2_game_data::AssetId;

use crate::authored_drag_gesture::{UiAuthoredDragCommand, UiAuthoredDragHandle};
use crate::authored_globe_presentation_types::UiGlobeBiomeAsset;
use crate::authored_globe_presentation_types::UiGlobeBiomeModel;
use crate::authored_globe_presentation_types::UiGlobeDrag;
use crate::authored_globe_presentation_types::UiGlobePresentation;
use crate::authored_globe_presentation_types::UiGlobeSceneAssets;
use crate::authored_globe_presentation_types::{GlobeHoveredMarker, GlobeSpin};
use crate::authored_grid_layout_projection::apply_authored_grid_record_to_bevy_node_layout;
use crate::authored_grid_layout_projection::apply_authored_list_scroll_axis_to_bevy_node_overflow;
use crate::authored_grid_layout_projection::authored_grid_record_owns_child_layout;
use crate::authored_rail_camera_presentation::UiRailCameraPresentation;
use crate::authored_text_edit_bevy_adaptation::UiTextEditPolicy;
use crate::authored_toggle_selection_transitions::UiAuthoredToggleGroupSelectionPolicy;
use crate::authored_tooltip_presentation::UiTooltipPolicy;
use crate::authored_tree_expansion_and_row_indentation::UiAuthoredTreeExpansion;
use crate::authored_tree_expansion_and_row_indentation::UiAuthoredTreeItem;
use crate::authored_ui_canvas_scaling_and_clipping::UiPhysicalAspect;
use crate::authored_window_interaction::UiAuthoredWindowInteractionPolicy;
use crate::held_button_repeat::UiButtonHold;
use crate::scenario_objective_status_visual_classification::UiScenarioObjectiveStatusVisualClassification;
use crate::slider::UiSliderAxis;
use crate::slider::UiSliderPolicy;
use crate::slider::UiSliderValuePresentation;
use openzt2_game_assets::ui_document::ui_document_asset_types_and_borrowing_queries::UiDocumentAsset;
use openzt2_interface_state::ui::authored_button_runtime_policy::UiButtonPolicy;
use openzt2_interface_state::ui::authored_reusable_list_and_table_runtime_types::UiListPolicy;
use openzt2_interface_state::ui::authored_reusable_list_and_table_runtime_types::UiTablePolicy;
use openzt2_interface_state::ui::authored_reusable_list_and_table_runtime_types::UiTypeListPolicy;
use openzt2_interface_state::ui::authored_ui_action_projection_components::UiActionSource;
use openzt2_interface_state::ui::authored_ui_integer_range_components::UiMaximum;
use openzt2_interface_state::ui::authored_ui_integer_range_components::UiMinimum;
use openzt2_interface_state::ui::authored_ui_node_projection_components::UiDocumentOwner;
use openzt2_interface_state::ui::authored_ui_node_projection_components::UiValue;
use openzt2_interface_state::ui::authored_ui_selection_state::UiSelected;
use openzt2_zoo_state::construction::selected_biome_ui_projection::UiBiomePanel;

pub(super) fn project_widget(
    commands: &mut Commands,
    entity: Entity,
    document_root: Entity,
    document: &UiDocumentAsset,
    node: &UiNodeDefinition,
    node_index: u32,
    localization: openzt2_game_assets::localization::loaded_localization_queries::LoadedLocalizationView<
        '_,
    >,
    viewport: Option<(UiSliderAxis, Entity)>,
) {
    let data = document.canonical_ui_document();
    if let Some(visual) = node.scenario_objective_visual.as_ref() {
        let visual = match visual {
            ObjectiveStatusFilter::All => ObjectiveStatusFilter::All,
            ObjectiveStatusFilter::Success => ObjectiveStatusFilter::Success,
            ObjectiveStatusFilter::Failure => ObjectiveStatusFilter::Failure,
            ObjectiveStatusFilter::Neutral => ObjectiveStatusFilter::Neutral,
        };
        commands
            .entity(entity)
            .insert(UiScenarioObjectiveStatusVisualClassification(visual));
    }
    let recipe = &node.widget;
    let wraps_text = node.style.flags.0 & UiStyleFlags::WRAP_TEXT.0 != 0;
    match recipe {
        UiWidgetRecord::Plain => {}
        UiWidgetRecord::BiomePanel { biome } => {
            commands.entity(entity).insert(UiBiomePanel {
                biome: AssetId(biome.0),
            });
        }
        UiWidgetRecord::Text(text) => {
            apply_authored_text_record_to_bevy_text_and_layout(
                commands,
                entity,
                text,
                localization,
                wraps_text,
            );
        }
        UiWidgetRecord::Grid(grid) => {
            apply_authored_grid_record_to_bevy_node_layout(commands, entity, grid);
        }
        UiWidgetRecord::Button(button) => {
            let selectable = matches!(node.kind, UiNodeKind::Toggle);
            let initially_selected =
                normalize_authored_toggle_set_initial_selection_to_one_direct_child(
                    data,
                    node_index,
                    button.initially_selected,
                );
            let auto_size = button.auto_size;
            let minimum_height = button.minimum_height;
            let authored_extent = node.rect.map(|value| value);
            commands.entity(entity).insert((
                UiButtonPolicy {
                    auto_size,
                    minimum_height,
                    selectable,
                    sticky: button.sticky,
                    repress: button.repress,
                    repeat_delay_seconds: button.repeat_delay_seconds,
                    hold_change: button.hold_change,
                    hold_interval_cap_seconds: button.hold_interval_cap_seconds,
                    activation_data: AssetId(button.activation_data.0),
                    child_button: AssetId(button.child_button.0),
                    hover_child: AssetId(button.hover_child.0),
                },
                UiButtonHold::default(),
            ));
            commands
                .entity(entity)
                .entry::<Node>()
                .and_modify(move |mut value| {
                    value.min_height = px(minimum_height.max(0) as f32);
                    if auto_size {
                        if authored_extent[2] <= 0.0 {
                            value.width = Val::Auto;
                        }
                        if authored_extent[3] <= 0.0 {
                            value.height = Val::Auto;
                        }
                    }
                });
            if selectable {
                commands
                    .entity(entity)
                    .insert(UiSelected(initially_selected));
                // When shipped UI selects more than one member of a toggle set,
                // the first member wins (the construction panel opens on
                // fences) and the later members' authored Off actions still
                // run. Keep that transition when normalizing to one child.
                if button.initially_selected && !initially_selected {
                    commands.entity(entity).insert(
                        crate::authored_ui_change_activation_dispatch::UiInitialDeselection,
                    );
                }
            }
            if matches!(node.kind, UiNodeKind::FullscreenButton) {
                commands.entity(entity).insert(
                    crate::authored_click_catcher_lifetime::AuthoredClickCatcherLifetime(
                        button.delayed_activation_seconds,
                    ),
                );
            }
        }
        UiWidgetRecord::List {
            grid,
            source,
            row_document,
            opener_node,
            drop_list_display_node,
            ..
        } => {
            apply_authored_grid_record_to_bevy_node_layout(commands, entity, grid);
            apply_authored_list_scroll_axis_to_bevy_node_overflow(commands, entity, grid);
            commands.entity(entity).insert((
                UiListPolicy {
                    row_document: document
                        .nested_ui_document_handle(AssetId(row_document.0))
                        .cloned(),
                    opener_node: AssetId(opener_node.0),
                    drop_list_display_node: AssetId(drop_list_display_node.0),
                    source: *source,
                },
                ScrollPosition::default(),
            ));
            match source {
                UiWidgetLiveCollectionSource::ZooMessageLatest => {
                    commands
                        .entity(entity)
                        .insert(crate::authored_zoo_message_lists::UiZooMessageList::Latest);
                }
                UiWidgetLiveCollectionSource::ZooMessageHistory => {
                    commands
                        .entity(entity)
                        .insert(crate::authored_zoo_message_lists::UiZooMessageList::History);
                }
                _ => {}
            }
            // These lists are authored toggle sets: selecting one row
            // deselects the others.
            if matches!(
                *source,
                UiWidgetLiveCollectionSource::Campaigns
                    | UiWidgetLiveCollectionSource::CampaignScenarios
                    | UiWidgetLiveCollectionSource::SavedGameSlots
            ) {
                commands
                    .entity(entity)
                    .insert(UiAuthoredToggleGroupSelectionPolicy::new(false));
            }
        }
        UiWidgetRecord::Slider(slider) => {
            let mut track_node = Node {
                position_type: PositionType::Absolute,
                ..default()
            };
            apply_authored_node_region_to_bevy_node(&mut track_node, &slider.thumb_region);
            let track = commands
                .spawn((
                    Name::new("authored slider thumb-centre interval"),
                    track_node,
                    Pickable::IGNORE,
                    UiDocumentOwner(document_root),
                    ChildOf(entity),
                ))
                .id();
            let axis = viewport.map_or_else(
                || match slider.axis {
                    UiAxis::X => UiSliderAxis::Horizontal,
                    UiAxis::Y => UiSliderAxis::Vertical,
                    UiAxis::Both => UiSliderAxis::Both,
                },
                |(axis, _)| axis,
            );
            commands.entity(entity).insert((
                UiSliderPolicy {
                    increment: slider.increment,
                    minimum_thumb_size: slider.minimum_thumb_size,
                    axis,
                },
                crate::slider::UiSliderPresentation {
                    thumb: slider.thumb,
                    track,
                },
                crate::slider::AuthoredSliderGrab::default(),
            ));
            if let Some((_, viewport)) = viewport {
                commands
                    .entity(entity)
                    .insert(crate::authored_scrollbar::ScrollsViewport(viewport));
            } else {
                commands.entity(entity).insert((
                    UiMinimum(slider.minimum as i64),
                    UiMaximum(slider.maximum as i64),
                    UiValue(slider.initial as i64),
                ));
                if slider.thumb == AssetId::default()
                    && slider.thumb_region == node.region
                    && slider.minimum.abs().max(slider.maximum.abs()) <= 16_777_216.0
                {
                    use bevy::ui_widgets::{
                        Slider, SliderOrientation, SliderRange, SliderStep, SliderValue, TrackClick,
                    };
                    let orientation = match axis {
                        UiSliderAxis::Horizontal => SliderOrientation::Horizontal,
                        UiSliderAxis::Vertical => SliderOrientation::Vertical,
                        UiSliderAxis::Both => SliderOrientation::Auto,
                    };
                    commands.entity(entity).insert((
                        Slider {
                            orientation,
                            track_click: TrackClick::Snap,
                        },
                        SliderRange::new(
                            slider.minimum.min(slider.maximum),
                            slider.maximum.max(slider.minimum),
                        ),
                        SliderValue(slider.initial),
                        SliderStep(slider.increment.abs()),
                    ));
                }
            }
            if AssetId(slider.field.0) != AssetId::default() {
                commands.entity(entity).insert(UiSliderValuePresentation {
                    label: AssetId(slider.field.0),
                    minimum_width: slider.field_format.minimum_width,
                    decimal_places: slider.field_format.decimal_places,
                    percent_suffix: slider.field_format.percent_suffix,
                });
            }
        }
        UiWidgetRecord::Drag {
            minimum_width,
            maximum_width,
            minimum_height,
            maximum_height,
            bounded,
        } => {
            commands.entity(entity).insert(UiAuthoredDragHandle {
                minimum_size: IVec2::new(*minimum_width, *minimum_height),
                maximum_size: IVec2::new(*maximum_width, *maximum_height),
                bounded: *bounded,
            });
        }
        UiWidgetRecord::DragCommand {
            operation,
            axis,
            flip_axis,
        } => {
            commands.entity(entity).insert(UiAuthoredDragCommand {
                operation: *operation,
                axis: match axis {
                    UiAxis::X => UiSliderAxis::Horizontal,
                    UiAxis::Y => UiSliderAxis::Vertical,
                    UiAxis::Both => UiSliderAxis::Both,
                },
                flip_axis: *flip_axis,
            });
        }
        UiWidgetRecord::Graph {
            graph_type,
            forced_minimum_y,
            forced_maximum_y,
            force_y_values,
            x_labels,
            y_labels,
        } => {
            let policy = crate::graph_presentation_types::UiGraphPresentationPolicy::from_authored_graph_definition(
                graph_type,
                *forced_minimum_y,
                *forced_maximum_y,
                *force_y_values,
                *x_labels,
                *y_labels,
            );
            commands.entity(entity).insert((
                policy,
                openzt2_interface_state::information::information_graph_types::InformationGraph {
                    graph: None,
                    label_document: None,
                    graph_type: policy.graph_type(),
                },
            ));
        }
        UiWidgetRecord::TypeList {
            grid,
            included_kinds,
            excluded_kinds,
            row_document,
            filters,
        } => {
            apply_authored_grid_record_to_bevy_node_layout(commands, entity, grid);
            apply_authored_list_scroll_axis_to_bevy_node_overflow(commands, entity, grid);
            commands.entity(entity).insert((
                UiTypeListPolicy {
                    included_kinds: included_kinds.clone(),
                    excluded_kinds: excluded_kinds.clone(),
                    filters: filters.clone(),
                },
                UiListPolicy {
                    row_document: document
                        .nested_ui_document_handle(AssetId(row_document.0))
                        .cloned(),
                    opener_node: AssetId::default(),
                    drop_list_display_node: AssetId::default(),
                    source: UiWidgetLiveCollectionSource::Unbound,
                },
                ScrollPosition::default(),
                // Native `ZTUITypeList` derives from `UIToggleSet` and carries
                // its own selection fact; the authored `UI_ACTIVATE_ON`/`OFF`
                // list events drive it and the catalogue selection consumer
                // reads its activation.
                UiSelected(false),
            ));
        }
        UiWidgetRecord::AdoptionList { grid, row_document } => {
            apply_authored_grid_record_to_bevy_node_layout(commands, entity, grid);
            apply_authored_list_scroll_axis_to_bevy_node_overflow(commands, entity, grid);
            commands.entity(entity).insert((
                UiTablePolicy::AdoptionList,
                UiListPolicy {
                    row_document: document
                        .nested_ui_document_handle(AssetId(row_document.0))
                        .cloned(),
                    opener_node: AssetId::default(),
                    drop_list_display_node: AssetId::default(),
                    source: UiWidgetLiveCollectionSource::Unbound,
                },
                UiAuthoredToggleGroupSelectionPolicy::new(false),
                ScrollPosition::default(),
            ));
        }
        UiWidgetRecord::FinanceList {
            grid,
            row_document,
            labels,
        } => {
            apply_authored_grid_record_to_bevy_node_layout(commands, entity, grid);
            apply_authored_list_scroll_axis_to_bevy_node_overflow(commands, entity, grid);
            commands.entity(entity).insert((
                if *labels {
                    UiTablePolicy::FinanceLabels
                } else {
                    UiTablePolicy::FinanceValues
                },
                UiListPolicy {
                    row_document: document
                        .nested_ui_document_handle(AssetId(row_document.0))
                        .cloned(),
                    opener_node: AssetId::default(),
                    drop_list_display_node: AssetId::default(),
                    source: UiWidgetLiveCollectionSource::Unbound,
                },
                ScrollPosition::default(),
            ));
        }
        UiWidgetRecord::CatalogueDetails { grid } => {
            apply_authored_grid_record_to_bevy_node_layout(commands, entity, grid);
        }
        UiWidgetRecord::WorldMap { .. } => {
            commands.entity(entity).insert(UiTablePolicy::WorldMap);
        }
        UiWidgetRecord::MultiIcon { .. } => {
            commands.entity(entity).insert((
                crate::authored_multi_icon_presentation::UiMultiIconPolicy,
                openzt2_interface_state::ui::authored_ui_visual_types::UiSourceRect([0, 0, -1, -1]),
            ));
            let has_value_binding = node
                .bindings
                .iter()
                .any(|binding| matches!(binding, UiNodePropertyBinding::IntegerValue(_)));
            if !has_value_binding {
                commands.entity(entity).insert(UiValue(0));
            }
        }
        UiWidgetRecord::TimedSequence {
            events,
            starts_active,
        } => {
            use crate::authored_timed_action_sequence::{
                UiAuthoredTimedActionSequencePlayback, UiTimedActionDue,
            };
            let mut steps = Vec::with_capacity(events.len());
            for (index, event) in events.iter().enumerate() {
                let proxy = commands
                    .spawn((
                        Name::new("timed UI action"),
                        UiDocumentOwner(document_root),
                        ChildOf(entity),
                        UiTimedActionDue(event.due),
                    ))
                    .id();
                steps.push(proxy);
                super::authored_node_action_routing_projection::insert_authored_action_proxy_route(
                    commands,
                    proxy,
                    UiActionSource::TimedEvent {
                        node: node_index,
                        event: index as u32,
                    },
                    &event.action,
                );
            }
            commands
                .entity(entity)
                .insert(UiAuthoredTimedActionSequencePlayback::new(
                    document_root,
                    steps.into_boxed_slice(),
                    events.first().is_some_and(|step| step.due.is_zero()),
                    *starts_active,
                ));
        }
        UiWidgetRecord::Tool { tool } => {
            if *tool != AssetId::default() {
                commands
                    .entity(entity)
                    .insert(crate::authored_tool_and_edit_widget::UiToolCommand(*tool));
            }
        }
        UiWidgetRecord::XmlEdit { document: edit } => {
            if let Some(asset) = document.nested_ui_document_handle(*edit) {
                commands.entity(entity).insert(
                    crate::authored_tool_and_edit_widget::UiEditDocument(asset.clone()),
                );
            }
        }
        UiWidgetRecord::TextEdit {
            text,
            maximum_length,
            legal_filename_only,
            highlight,
            cursor_on_seconds,
            cursor_off_seconds,
            ..
        } => {
            apply_authored_text_record_to_bevy_text_and_layout(
                commands,
                entity,
                text,
                localization,
                wraps_text,
            );
            commands
                .entity(entity)
                .insert(UiTextEditPolicy::from_authored_text_edit_definition(
                    (*maximum_length).max(0) as u32,
                    *legal_filename_only,
                    *highlight,
                    *cursor_on_seconds,
                    *cursor_off_seconds,
                ));
        }
        UiWidgetRecord::ToggleSet {
            grid,
            allow_repress,
            initial_column: _,
            source,
        } => {
            if authored_grid_record_owns_child_layout(grid) {
                apply_authored_grid_record_to_bevy_node_layout(commands, entity, grid);
            }
            commands
                .entity(entity)
                .insert(UiAuthoredToggleGroupSelectionPolicy::new(*allow_repress));
            let source = *source;
            if source != UiWidgetLiveCollectionSource::Unbound || grid.rows < 0 || grid.columns < 0
            {
                apply_authored_list_scroll_axis_to_bevy_node_overflow(commands, entity, grid);
                commands.entity(entity).insert((
                    UiListPolicy {
                        row_document: None,
                        opener_node: AssetId::default(),
                        drop_list_display_node: AssetId::default(),
                        source,
                    },
                    ScrollPosition::default(),
                ));
            }
        }
        UiWidgetRecord::Globe(globe) => {
            commands.entity(entity).insert((
                UiPhysicalAspect,
                UiGlobePresentation {
                    primary_model: AssetId(globe.primary_model.0),
                    primary_translation: Vec3::from_array(globe.primary_translation),
                    clouds_model: AssetId(globe.clouds_model.0),
                    clouds_translation: Vec3::from_array(globe.clouds_translation),
                    dot_model: AssetId(globe.dot_model.0),
                    dot_translation: Vec3::from_array(globe.dot_translation),
                    selected_dot_model: AssetId(globe.selected_dot_model.0),
                    selected_dot_translation: Vec3::from_array(globe.selected_dot_translation),
                    pointer_model: AssetId(globe.pointer_model.0),
                    selection_rotate_speed: globe.selection_rotate_speed,
                    mouse_increment: globe.mouse_increment,
                    mouse_down_friction: globe.mouse_down_friction,
                    mouse_up_friction: globe.mouse_up_friction,
                    friction_transition_seconds: globe.friction_transition_seconds,
                    move_seconds: globe.move_seconds,
                    scream_threshold: globe.scream_threshold,
                    scream_delay_seconds: globe.scream_delay_seconds,
                    scream_audio: globe.scream_audio,
                    dot_highlight_rgba: globe.dot_highlight_rgba,
                    dot_highlight_cursor: AssetId(globe.dot_highlight_cursor.0),
                },
                UiGlobeSceneAssets {
                    primary: document
                        .scene_prefab_handle(AssetId(globe.primary_model.0))
                        .cloned(),
                    clouds: document
                        .scene_prefab_handle(AssetId(globe.clouds_model.0))
                        .cloned(),
                    dot: document
                        .scene_prefab_handle(AssetId(globe.dot_model.0))
                        .cloned(),
                    selected_dot: document
                        .scene_prefab_handle(AssetId(globe.selected_dot_model.0))
                        .cloned(),
                },
                UiGlobeDrag::default(),
                GlobeHoveredMarker::default(),
                GlobeSpin::default(),
                Pickable::default(),
            ));
            commands
                .entity(entity)
                .entry::<Node>()
                .and_modify(|mut node| {
                    node.overflow = Overflow::clip();
                });
            for model in &globe.biome_models {
                let mut biome_entity = commands.spawn((
                    UiGlobeBiomeModel {
                        biome: AssetId(model.biome.0),
                    },
                    Visibility::Hidden,
                    ChildOf(entity),
                ));
                if let Some(handle) = document
                    .scene_prefab_handle(AssetId(model.model.0))
                    .cloned()
                {
                    biome_entity.insert(UiGlobeBiomeAsset(handle));
                }
            }
        }
        UiWidgetRecord::RailCamera { scene } => {
            if let Some(handle) = document.scene_prefab_handle(AssetId(scene.0)).cloned() {
                commands.entity(entity).insert((
                    UiPhysicalAspect,
                    UiRailCameraPresentation(handle),
                    UiSelected(true),
                ));
            }
        }
        UiWidgetRecord::Tooltip {
            text,
            presentation,
            floating,
            autohide,
            offset,
            appear_seconds,
            display_seconds,
        } => {
            apply_authored_text_record_to_bevy_text_and_layout(
                commands,
                entity,
                text,
                localization,
                wraps_text,
            );
            commands
                .entity(entity)
                .entry::<Text>()
                .or_insert(Text::new(""));
            if *floating {
                commands
                    .entity(entity)
                    .entry::<Node>()
                    .and_modify(|mut node| {
                        // The native UITooltip recomputes its floating extent
                        // when the resolved short text changes. Its authored
                        // BFRect is a visual source rectangle, not a fixed
                        // 128-pixel label width.
                        node.width = Val::Auto;
                        node.height = Val::Auto;
                    });
            }
            let global = matches!(data.role, UiDocumentRole::ApplicationRoot);
            if global {
                commands.entity(entity).insert(GlobalZIndex(i32::MAX - 2));
            }
            commands.entity(entity).insert(UiTooltipPolicy {
                presentation: match presentation {
                    UiTooltipPresentation::Name => UiTooltipPresentation::Name,
                    UiTooltipPresentation::Short => UiTooltipPresentation::Short,
                    UiTooltipPresentation::Long => UiTooltipPresentation::Long,
                    UiTooltipPresentation::Help => UiTooltipPresentation::Help,
                },
                global,
                floating: *floating,
                autohide: *autohide,
                offset: [offset[0], offset[1]],
                appear_seconds: *appear_seconds,
                display_seconds: *display_seconds,
            });
        }
        UiWidgetRecord::TreeElement {
            grid,
            item,
            expanded,
            selected,
        } => {
            apply_authored_grid_record_to_bevy_node_layout(commands, entity, grid);
            commands.entity(entity).insert((
                UiAuthoredTreeItem::from_authored_item_and_initial_depth(AssetId(item.0), 0),
                UiAuthoredTreeExpansion::from_authored_expansion(*expanded),
                UiSelected(*selected),
            ));
        }
        UiWidgetRecord::Window {
            draggable,
            horizontal,
            vertical,
            wheel_scroll,
            ..
        } => {
            let horizontal = *horizontal;
            let vertical = *vertical;
            commands.entity(entity).insert(
                UiAuthoredWindowInteractionPolicy::from_authored_interaction(
                    *draggable,
                    *wheel_scroll,
                ),
            );
            commands.entity(entity).insert(ScrollPosition::default());
            commands
                .entity(entity)
                .entry::<Node>()
                .and_modify(move |mut value| {
                    value.overflow = Overflow {
                        x: if horizontal {
                            OverflowAxis::Scroll
                        } else {
                            OverflowAxis::Clip
                        },
                        y: if vertical {
                            OverflowAxis::Scroll
                        } else {
                            OverflowAxis::Clip
                        },
                    };
                });
        }
    }
}

fn normalize_authored_toggle_set_initial_selection_to_one_direct_child(
    document: &openzt2_game_data::ui_document::document::UiDocument,
    node_index: u32,
    initially_selected: bool,
) -> bool {
    if !initially_selected {
        return false;
    }
    let Some(node) = document.nodes.get(node_index as usize) else {
        return true;
    };
    let Some(parent) = document.nodes.get(node.parent as usize) else {
        return true;
    };
    if !matches!(parent.widget, UiWidgetRecord::ToggleSet { .. }) {
        return true;
    }
    parent
        .children
        .iter()
        .find(|&&candidate_index| {
            document
                .nodes
                .get(candidate_index as usize)
                .is_some_and(|candidate| {
                    matches!(candidate.kind, UiNodeKind::Toggle)
                        && matches!(
                            &candidate.widget,
                            UiWidgetRecord::Button(button) if button.initially_selected
                        )
                })
        })
        .is_none_or(|&selected_index| selected_index == node_index)
}
