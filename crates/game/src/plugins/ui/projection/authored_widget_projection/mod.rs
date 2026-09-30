use super::authored_node_layout_projection::apply_authored_node_region_to_bevy_node;
use super::authored_text_projection::apply_authored_text_record_to_bevy_text_and_layout;
use bevy::prelude::*;
use openzt2_game_data::ui_document::action::UiTrigger;
use openzt2_game_data::{
    ui_document::{
        action::scenarios::ObjectiveStatusFilter,
        document::UiDocumentRole,
        node::{UiNodeDefinition, UiNodeKind},
        node_property_binding::UiNodePropertyBinding,
        node_style::UiStyleFlags,
        widget::UiWidgetRecord,
        widget_control::{UiAxis, UiDragOperation, UiTooltipPresentation},
        widget_live_collection::UiWidgetLiveCollectionSource,
    },
    AssetId,
};

use crate::assets::ui_document::ui_document_asset_types_and_borrowing_queries::UiDocumentAsset;
use crate::plugins::construction::selected_biome_ui_projection::UiBiomePanel;
use crate::plugins::input::input_types::ActionSource;
use crate::plugins::ui::authored_button_attention_pulse_presentation::UiAuthoredButtonAttentionPulse;
use crate::plugins::ui::authored_button_runtime_policy::UiButtonPolicy;
use crate::plugins::ui::authored_credits_sequence_presentation::UiAuthoredCreditsSequencePlayback;
use crate::plugins::ui::authored_drag_gesture::UiAuthoredDragGesturePolicy;
use crate::plugins::ui::authored_globe_presentation_types::{
    UiGlobeBiomeAsset, UiGlobeBiomeModel, UiGlobeDrag, UiGlobePresentation, UiGlobeSceneAssets,
};
use crate::plugins::ui::authored_grid_layout_projection::{
    apply_authored_grid_record_to_bevy_node_layout,
    apply_authored_list_scroll_axis_to_bevy_node_overflow, authored_grid_record_owns_child_layout,
};
use crate::plugins::ui::authored_rail_camera_presentation::UiRailCameraPresentation;
use crate::plugins::ui::authored_reusable_list_and_table_runtime_types::{
    UiListPolicy, UiTablePolicy, UiTypeListPolicy,
};
use crate::plugins::ui::authored_text_edit_bevy_adaptation::UiTextEditPolicy;
use crate::plugins::ui::authored_toggle_selection_transitions::UiAuthoredToggleGroupSelectionPolicy;
use crate::plugins::ui::authored_tooltip_presentation::UiTooltipPolicy;
use crate::plugins::ui::authored_tree_expansion_and_row_indentation::{
    UiAuthoredTreeExpansion, UiAuthoredTreeItem,
};
use crate::plugins::ui::authored_ui_action_projection_components::{
    UiActionSource, UiPresentationActions,
};
use crate::plugins::ui::authored_ui_activation_contracts::UiNodeActivated;
use crate::plugins::ui::authored_ui_canvas_scaling_and_clipping::UiPhysicalAspect;
use crate::plugins::ui::authored_ui_change_activation_dispatch::UiPreviousSelection;
use crate::plugins::ui::authored_ui_integer_range_components::{UiMaximum, UiMinimum};
use crate::plugins::ui::authored_ui_node_projection_components::{UiDocumentOwner, UiValue};
use crate::plugins::ui::authored_ui_selection_state::UiSelected;
use crate::plugins::ui::authored_window_interaction::UiAuthoredWindowInteractionPolicy;
use crate::plugins::ui::held_button_repeat::UiButtonHold;
use crate::plugins::ui::scenario_objective_status_visual_classification::UiScenarioObjectiveStatusVisualClassification;
use crate::plugins::ui::slider::{UiSliderAxis, UiSliderPolicy, UiSliderValuePresentation};

pub(super) fn project_widget(
    commands: &mut Commands,
    entity: Entity,
    document_root: Entity,
    document: &UiDocumentAsset,
    node: &UiNodeDefinition,
    node_index: u32,
    localization: crate::assets::localization::loaded_localization_queries::LoadedLocalizationView<
        '_,
    >,
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
                commands.entity(entity).insert((
                    UiSelected(initially_selected),
                    UiPreviousSelection::from_current_selection(initially_selected),
                ));
                // Blue Fang toggle sets apply initially selected members in
                // source order. When malformed shipped UI selects more than
                // one member, the last member wins and the earlier members'
                // authored Off actions still run. Keep that presentation
                // transition when normalizing the selected fact to one child.
                if button.initially_selected && !initially_selected {
                    commands.queue(move |world: &mut World| {
                        world.write_message(UiNodeActivated {
                            source: ActionSource::System,
                            node: entity,
                            trigger: UiTrigger::Off,
                        });
                    });
                }
            }
            if matches!(node.kind, UiNodeKind::FullscreenButton) {
                commands.entity(entity).insert(
                    crate::plugins::ui::full_canvas_click_catcher::UiFullCanvasClickCatcher::new(
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
            if !matches!(node.kind, UiNodeKind::DropList) {
                commands.entity(entity).insert(Interaction::None);
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
            commands.entity(entity).insert((
                UiMinimum(slider.minimum as i64),
                UiMaximum(slider.maximum as i64),
                UiValue(slider.initial as i64),
                UiSliderPolicy {
                    increment: slider.increment,
                    minimum_thumb_size: slider.minimum_thumb_size,
                    axis: match &slider.axis {
                        UiAxis::X => UiSliderAxis::Horizontal,
                        UiAxis::Y => UiSliderAxis::Vertical,
                        UiAxis::Both => UiSliderAxis::Both,
                    },
                },
                crate::plugins::ui::slider::UiSliderPresentation {
                    thumb: AssetId(slider.thumb.0),
                    track,
                },
            ));
            if AssetId(slider.field.0) != AssetId::default() {
                commands.entity(entity).insert(UiSliderValuePresentation {
                    label: AssetId(slider.field.0),
                    minimum_width: slider.field_format.minimum_width,
                    decimal_places: slider.field_format.decimal_places,
                    percent_suffix: slider.field_format.percent_suffix,
                });
            }
        }
        // The canonical record retains the authored bounds. No drag
        // procedure consumes them yet, so do not project an unread ECS copy.
        UiWidgetRecord::Drag { .. } => {}
        UiWidgetRecord::DragCommand {
            operation,
            axis,
            flip_axis,
        } => {
            commands
                .entity(entity)
                .insert(UiAuthoredDragGesturePolicy::from_authored_gesture(
                    match operation {
                        UiDragOperation::Move => UiDragOperation::Move,
                        UiDragOperation::Resize => UiDragOperation::Resize,
                        UiDragOperation::Scroll => UiDragOperation::Scroll,
                    },
                    match axis {
                        UiAxis::X => UiSliderAxis::Horizontal,
                        UiAxis::Y => UiSliderAxis::Vertical,
                        UiAxis::Both => UiSliderAxis::Both,
                    },
                    *flip_axis,
                ));
        }
        UiWidgetRecord::Graph {
            graph_type,
            forced_minimum_y,
            forced_maximum_y,
            force_y_values,
            x_labels,
            y_labels,
        } => {
            let policy = crate::plugins::ui::graph_presentation_types::UiGraphPresentationPolicy::from_authored_graph_definition(
                graph_type,
                *forced_minimum_y,
                *forced_maximum_y,
                *force_y_values,
                *x_labels,
                *y_labels,
            );
            commands.entity(entity).insert((
                policy,
                crate::plugins::information::information_graph_types::InformationGraph {
                    graph: None,
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
                Interaction::None,
                // Native `ZTUITypeList` derives from `UIToggleSet` and carries
                // its own selection fact; the authored `UI_ACTIVATE_ON`/`OFF`
                // list events drive it and the catalogue selection consumer
                // reads its activation.
                UiSelected(false),
                UiPreviousSelection::from_current_selection(false),
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
                Interaction::None,
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
                Interaction::None,
            ));
        }
        UiWidgetRecord::CatalogueDetails { grid } => {
            apply_authored_grid_record_to_bevy_node_layout(commands, entity, grid);
            commands
                .entity(entity)
                .insert(crate::plugins::information::catalogue_types::CatalogueDetails::default());
        }
        UiWidgetRecord::WorldMap { .. } => {
            commands.entity(entity).insert(UiTablePolicy::WorldMap);
        }
        UiWidgetRecord::MultiIcon { .. } => {
            commands
                .entity(entity)
                .insert(crate::plugins::ui::authored_multi_icon_presentation::UiMultiIconPolicy);
            let has_value_binding = node
                .bindings
                .iter()
                .any(|binding| matches!(binding, UiNodePropertyBinding::IntegerValue(_)));
            if !has_value_binding {
                commands.entity(entity).insert(UiValue(0));
            }
        }
        UiWidgetRecord::LocalizedCountdown(countdown) => {
            commands.entity(entity).insert(
                crate::plugins::ui::localized_countdown_presentation::UiLocalizedCountdownPresentation::new(
                    AssetId(countdown.text_node.0),
                    AssetId(countdown.localization_key.0),
                    countdown.start_count,
                    countdown.tick_interval_ms,
                    AssetId(countdown.completion_target.0),
                ),
            );
        }
        UiWidgetRecord::TimedSequence { events } => {
            let actions = events
                .iter()
                .enumerate()
                .map(|(event, _)| {
                    commands
                        .spawn((
                            Name::new("timed UI action"),
                            UiDocumentOwner(document_root),
                            UiPresentationActions(UiActionSource::TimedEvent {
                                node: node_index,
                                event: event as u32,
                            }),
                            ChildOf(entity),
                        ))
                        .id()
                })
                .collect();
            commands.entity(entity).insert(
                crate::plugins::ui::authored_timed_action_sequence::UiAuthoredTimedActionSequencePlayback::new(
                    node_index,
                    actions,
                ),
            );
        }
        UiWidgetRecord::CreditsSequence {
            repeat_after_ms, ..
        } => {
            commands.entity(entity).insert(
                UiAuthoredCreditsSequencePlayback::from_authored_sequence(
                    node_index,
                    *repeat_after_ms,
                ),
            );
        }
        UiWidgetRecord::EarthquakeTrigger
        | UiWidgetRecord::EarthquakeRunner
        | UiWidgetRecord::FirstPersonCountdown { .. }
        | UiWidgetRecord::Tool { .. }
        | UiWidgetRecord::XmlEdit { .. } => {}
        UiWidgetRecord::ButtonPulser {
            target,
            hidden_ms,
            visible_ms,
        } => {
            commands
                .entity(entity)
                .insert(UiAuthoredButtonAttentionPulse::from_authored_cadence(
                    AssetId(target.0),
                    *hidden_ms,
                    *visible_ms,
                ));
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
            if source != UiWidgetLiveCollectionSource::Unbound {
                apply_authored_list_scroll_axis_to_bevy_node_overflow(commands, entity, grid);
                commands.entity(entity).insert((
                    UiListPolicy {
                        row_document: None,
                        opener_node: AssetId::default(),
                        drop_list_display_node: AssetId::default(),
                        source,
                    },
                    ScrollPosition::default(),
                    Interaction::None,
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
                Interaction::None,
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
                UiPreviousSelection::from_current_selection(*selected),
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
            commands
                .entity(entity)
                .insert((ScrollPosition::default(), Interaction::None));
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
        .rev()
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
