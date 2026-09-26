use super::authored_hotkey_projection::project_authored_hotkey_action_proxies;
use super::authored_node_action_routing_projection::insert_authored_node_action_routing_components;
use super::authored_node_layout_projection::{
    authored_list_ancestor, project_authored_node_and_style_to_bevy_layout,
};
use super::authored_node_picking_projection::{
    authored_node_has_actions, authored_node_kind_owns_pointer_interaction,
    initial_bevy_pickable_for_authored_node,
};
use super::authored_node_property_binding_projection::{
    authored_constant_visibility_and_interaction_enabled_state,
    insert_authored_node_property_bindings,
};
use super::authored_node_visual_projection::project_authored_node_visuals_into_bevy_presentation;
use super::authored_widget_projection::project_widget;
use std::{collections::hash_map::RandomState, hash::BuildHasher};

use bevy::prelude::*;
use openzt2_game_data::{
    ui_document::{
        document::{UiDocument, UiDocumentRole},
        node::{UiNodeDefinition, UiNodeFlags, UiNodeKind},
        widget::UiWidgetRecord,
        widget_live_collection::UiWidgetLiveCollectionSource,
    },
    AssetId,
};

use crate::assets::ui_document::ui_document_asset_types_and_borrowing_queries::UiDocumentAsset;
use crate::plugins::ui::animation::{NotifyOnUiAnimationComplete, UiShowHideAnimation};
use crate::plugins::ui::authored_grid_layout_projection::{
    authored_grid_record_owns_child_layout, authored_unbounded_grid_packing_direction,
};
use crate::plugins::ui::authored_modal_presentation::UiAuthoredModalPresentation;
use crate::plugins::ui::authored_online_message_node_roles::{
    UiAuthoredOnlineMessageIcon, UiAuthoredOnlineMessageList, UiAuthoredOnlineMessageSurface,
    UiAuthoredOnlineMessageText,
};
use crate::plugins::ui::authored_tooltip_presentation::{
    UiDisplayNameKey, UiHelpTopicKey, UiLongTooltipKey, UiShortTooltipKey,
};
use crate::plugins::ui::authored_tree_expansion_and_row_indentation::UiAuthoredTreeHeader;
use crate::plugins::ui::authored_ui_canvas_scaling_and_clipping::{
    UiLogicalCanvas, UiPhysicalListRowHeight,
};
use crate::plugins::ui::authored_ui_change_activation_dispatch::{
    UiPreviousFocus, UiPreviousVisibility,
};
use crate::plugins::ui::authored_ui_focus_navigation_and_activation::UiSubmitTarget;
use crate::plugins::ui::authored_ui_focus_state::{UiFocusPresentation, UiFocusScope, UiFocusable};
use crate::plugins::ui::authored_ui_interaction_enabled_state::UiInteractionEnabled;
use crate::plugins::ui::authored_ui_layout_participation::UiAuthoredLayoutDisplay;
use crate::plugins::ui::authored_ui_pointer_activation::{
    UiPreviousPointerInteraction, UiPreviousPointerPressTimeSeconds,
};
use crate::plugins::ui::authored_ui_presentation_action_application::UiExclusiveHoverPresentation;
use crate::plugins::ui::cursor::UiCursorStyle;
use crate::plugins::ui::scrollbar_value_and_scroll_position_synchronization::UiScrollDecoration;
use crate::plugins::ui::slider::UiSliderAxis;
use crate::plugins::ui::{
    authored_ui_node_projection_components::UiDocumentOwner,
    authored_ui_node_projection_components::UiDocumentRoot,
    authored_ui_node_projection_components::UiNodeId,
    ui_document_lifecycle_contracts::ShowUiDocument,
};

pub(crate) fn project_document(
    commands: &mut Commands,
    request: &ShowUiDocument,
    document: &UiDocumentAsset,
    localization: crate::assets::localization::loaded_localization_queries::LoadedLocalizationView<
        '_,
    >,
    image_selection_index: Option<usize>,
    decorate_owner: bool,
) -> Entity {
    let data = document.canonical_ui_document();
    // A source file/directory skin chooses one alternative when the screen is
    // constructed and keeps that choice for the lifetime of the projected
    // Bevy subtree. `RandomState` supplies a process-randomized seed without a
    // retained skin manager or another gameplay random stream. The capture
    // harness may provide an explicit candidate index for reproducible visual
    // evidence.
    let image_selection_index = image_selection_index.unwrap_or_else(|| {
        RandomState::new().hash_one((data.id.0, request.owner.to_bits())) as usize
    });
    let mut entities = Vec::with_capacity(data.nodes.len());
    let mut root = None;
    let mut default_focus = None;
    let top_level = !matches!(&data.role, UiDocumentRole::Fragment);

    for (index, record) in data.nodes.iter().enumerate() {
        let entity = commands.spawn_empty().id();
        if index == 0 {
            root = Some(entity);
        }
        let document_root = root.expect("validated UI preorder starts at its root");
        entities.push(entity);

        let flags = record.flags.0;
        let (bound_visibility, bound_enabled) =
            authored_constant_visibility_and_interaction_enabled_state(&record.bindings);
        // The document root is Bevy's lifecycle/layout owner, not an authored
        // leaf whose source visibility bit should suppress its descendants.
        // Source lowering resolves the selected role surface and its initial
        // visibility; every authored descendant must retain that state.
        let visible = index == 0 || bound_visibility.unwrap_or(flags & UiNodeFlags::VISIBLE.0 != 0);
        let enabled = bound_enabled.unwrap_or(flags & UiNodeFlags::ENABLED.0 != 0);
        let focusable = flags & UiNodeFlags::FOCUSABLE.0 != 0;
        let style = &record.style;
        let text_color_style = if flags & UiNodeFlags::USE_LIST_BOX_COLOR.0 != 0 {
            authored_list_ancestor(document, index as u32).map_or(style, |ancestor| &ancestor.style)
        } else {
            style
        };
        let mut node = project_authored_node_and_style_to_bevy_layout(record, style, flags);
        let scroll_decoration = scroll_decoration(data, index);
        let parent_layout =
            parent_layout_ownership(data, record).filter(|_| scroll_decoration.is_none());
        if let Some(parent_layout) = parent_layout {
            flow_authored_row(
                &mut node,
                parent_layout.flow,
                parent_layout.column_width,
                parent_layout.row_height,
            );
        }
        let authored_display = node.display;
        if !visible {
            node.display = Display::None;
        }
        // Top-level documents overlay the shared authored canvas. Making them
        // relative flex children causes every additional modal to share and
        // shrink the owner's row with the HUD beneath it. Reusable fragments
        // remain relative because their owning list/grid supplies layout.
        if index == 0 {
            if top_level {
                node.position_type = PositionType::Absolute;
                node.flex_shrink = 0.0;
            } else {
                node.position_type = PositionType::Relative;
                // Reusable rows retain their authored extent. Overflow belongs
                // to the owning list's scroll area, not flex compression.
                node.flex_shrink = 0.0;
                node.left = Val::Auto;
                node.right = Val::Auto;
                node.top = Val::Auto;
                node.bottom = Val::Auto;
            }
        }
        let id = AssetId(record.id.0);
        commands.entity(entity).insert((
            Name::new(
                (!record.name.is_empty())
                    .then_some(record.name.as_str())
                    .unwrap_or("ui")
                    .to_owned(),
            ),
            node,
            if visible {
                Visibility::Inherited
            } else {
                Visibility::Hidden
            },
            ZIndex(record.z as i32),
            UiNodeId {
                index: index as u32,
                id,
            },
            UiDocumentOwner(document_root),
            UiAuthoredLayoutDisplay::from_authored_display(authored_display),
            initial_bevy_pickable_for_authored_node(record),
        ));
        if index == 0 && data.role == UiDocumentRole::InGamePersistentStatus {
            commands.entity(entity).insert(GlobalZIndex(1));
        }
        commands.entity(entity).insert(UiPreviousVisibility(false));
        if data.role == UiDocumentRole::MainMenu
            && record.animation.is_some()
            && record
                .name
                .trim()
                .to_ascii_lowercase()
                .ends_with(" pointer")
        {
            commands.entity(entity).insert(UiExclusiveHoverPresentation);
        }
        let cursor = AssetId(record.cursor.0);
        if cursor != AssetId::default() {
            commands.entity(entity).insert(UiCursorStyle(cursor));
        }
        if let Some((axis, owner_index)) = scroll_decoration {
            // Decorations belong to the viewport, not its scrolling content.
            // Bevy retains their authored parent-relative layout and clipping.
            commands
                .entity(entity)
                .insert(bevy::ui::IgnoreScroll(BVec2::TRUE));
            commands.entity(entity).insert(UiScrollDecoration {
                owner: entities[owner_index],
                axis,
            });
        }
        if flags & UiNodeFlags::ONLINE_MESSAGE_SURFACE.0 != 0 {
            commands
                .entity(entity)
                .insert(UiAuthoredOnlineMessageSurface);
        }
        if flags & UiNodeFlags::ONLINE_MESSAGE_LIST.0 != 0 {
            commands.entity(entity).insert(UiAuthoredOnlineMessageList);
        }
        if flags & UiNodeFlags::ONLINE_MESSAGE_TEXT.0 != 0 {
            commands.entity(entity).insert(UiAuthoredOnlineMessageText);
        }
        if flags & UiNodeFlags::ONLINE_MESSAGE_ICON.0 != 0 {
            commands.entity(entity).insert(UiAuthoredOnlineMessageIcon);
        }
        if flags & UiNodeFlags::MODAL.0 != 0 {
            commands
                .entity(entity)
                .insert(UiAuthoredModalPresentation::from_authored_modal_state(true));
        }
        insert_authored_node_action_routing_components(commands, entity, index as u32, record);

        if let Some(animation) = &record.animation {
            let duration_ms = animation.duration_ms as f32;
            let elapsed_ms = (animation.initial_ms as f32).min(duration_ms);
            let authored_animation = matches!(record.kind, UiNodeKind::Animation);
            commands.entity(entity).insert(UiShowHideAnimation {
                elapsed_ms,
                duration_ms,
                exit_rate: animation.exit_rate,
                delay_ms: animation.delay_ms as f32,
                delay_remaining_ms: if authored_animation {
                    animation.delay_ms as f32
                } else {
                    0.0
                },
                bob_ms: animation.bob_ms as f32,
                interpolation: animation.interpolation,
                forward: animation.initial_forward,
                running: duration_ms > 0.0
                    && (authored_animation || elapsed_ms > 0.0 && elapsed_ms < duration_ms),
                base_rect: record.rect.map(|value| value),
                start_rect: animation.start_rect.map(|value| value),
                end_rect: animation.end_rect.map(|value| value),
                animates_color: animation.animates_color,
                affects_text_color: animation.affects_text_color,
                start_color: animation.start_color,
                end_color: animation.end_color,
            });
        }
        if flags & UiNodeFlags::NOTIFY_ANIMATION_COMPLETED.0 != 0 {
            commands.entity(entity).insert(NotifyOnUiAnimationComplete);
        }

        if index == 0 {
            commands.entity(entity).insert((
                UiDocumentRoot {
                    document: request.document.clone(),
                },
                UiFocusScope::default(),
                UiPreviousFocus::default(),
            ));
            // The owner remains the lifecycle parent, but it must also
            // participate in Bevy UI's hierarchy. Bevy only lays out a Node
            // without a Node ancestor as a UI root; a marker-only parent would
            // otherwise exclude this entire document from layout.
            let owner = request.owner;
            commands.queue(move |world: &mut World| {
                let Ok(mut owner) = world.get_entity_mut(owner) else {
                    return;
                };
                if decorate_owner {
                    owner.insert((
                        Node {
                            width: percent(100.0),
                            height: percent(100.0),
                            justify_content: if top_level {
                                JustifyContent::Center
                            } else {
                                JustifyContent::Start
                            },
                            align_items: if top_level {
                                AlignItems::Center
                            } else {
                                AlignItems::Start
                            },
                            ..default()
                        },
                        Visibility::Inherited,
                        // Lifecycle owners participate in layout only. Without an
                        // explicit ignore marker, Bevy UI picking treats this
                        // full-window node as the top hit and never reaches the
                        // authored controls below it.
                        Pickable::IGNORE,
                    ));
                }
                owner.add_child(entity);
            });
            let logical_size = data.logical_size.map(|value| value);
            if top_level {
                commands.entity(entity).insert((
                    UiLogicalCanvas::from_logical_size(Vec2::new(1024.0, 768.0)),
                    UiTransform::IDENTITY,
                ));
            }
            commands
                .entity(entity)
                .entry::<Node>()
                .and_modify(move |mut node| {
                    let canvas = if top_level {
                        // Top-level documents share the original authored
                        // coordinate space. Per-document bounds can exceed it
                        // because they include off-canvas animation and layout
                        // extents; treating those bounds as a viewport stretches
                        // the interface and invalidates pointer projection.
                        [1024.0, 768.0]
                    } else {
                        logical_size
                    };
                    if canvas[0] > 0.0 {
                        node.width = px(canvas[0]);
                    }
                    if canvas[1] > 0.0 {
                        node.height = px(canvas[1]);
                    }
                });
        } else {
            let parent = entities[record.parent as usize];
            commands.entity(parent).add_child(entity);
        }

        if focusable {
            let order = if record.tab_order < 0 {
                index as u32
            } else {
                record.tab_order as u32
            };
            commands.entity(entity).insert((
                UiFocusable { order, enabled },
                UiInteractionEnabled(enabled),
                UiFocusPresentation::default(),
                Interaction::None,
            ));
            commands.entity(entity).insert((
                UiPreviousPointerInteraction::from_current_interaction(Interaction::None),
                UiPreviousPointerPressTimeSeconds::default(),
            ));
            default_focus.get_or_insert(entity);
        }
        let interactive = authored_node_has_actions(record)
            || authored_node_kind_owns_pointer_interaction(&record.kind);
        if interactive && !focusable {
            commands.entity(entity).insert((
                UiInteractionEnabled(enabled),
                Interaction::None,
                UiPreviousPointerInteraction::from_current_interaction(Interaction::None),
                UiPreviousPointerPressTimeSeconds::default(),
            ));
        }

        if matches!(
            &record.kind,
            UiNodeKind::Button | UiNodeKind::Toggle | UiNodeKind::Slider | UiNodeKind::Edit
        ) {
            commands.entity(entity).insert(Button);
        }
        if matches!(&record.kind, UiNodeKind::Edit) {
            commands.entity(entity).insert(UiSubmitTarget);
        }
        if matches!(&record.kind, UiNodeKind::Scroll) {
            commands.entity(entity).insert(ScrollPosition::default());
        }

        let help = &record.help;
        let mut node = commands.entity(entity);
        let name = AssetId(help.name.0);
        let short = AssetId(help.short.0);
        let long = AssetId(help.long.0);
        let topic = AssetId(help.help.0);
        if name != AssetId::default() {
            node.insert(UiDisplayNameKey(name));
        }
        if short != AssetId::default() {
            node.insert(UiShortTooltipKey(short));
        }
        if long != AssetId::default() {
            node.insert(UiLongTooltipKey(long));
        }
        if topic != AssetId::default() {
            node.insert(UiHelpTopicKey(topic));
        }
        insert_authored_node_property_bindings(commands, entity, &record.bindings);
        project_authored_node_visuals_into_bevy_presentation(
            commands,
            entity,
            data,
            id,
            &record.bindings,
            &record.visuals,
            &record.text,
            AssetId(record.text_format.0),
            style,
            text_color_style,
            document,
            localization,
            enabled,
            Some(image_selection_index),
            None,
        );
        project_widget(
            commands,
            entity,
            document_root,
            document,
            record,
            index as u32,
            localization,
        );
    }

    for record in &data.nodes {
        if !matches!(&record.widget, UiWidgetRecord::TreeElement { .. }) {
            continue;
        }
        let Some(header_index) = record.children.first().map(|&index| index as usize) else {
            continue;
        };
        if let Some(&header) = entities.get(header_index) {
            commands.entity(header).insert(UiAuthoredTreeHeader);
        }
    }

    if let Some(root) = root {
        commands.entity(root).insert(UiFocusScope {
            focused: default_focus,
            default: default_focus,
        });
        project_authored_hotkey_action_proxies(commands, root, document);
    }
    root.expect("validated UI document has a root")
}

/// Reuse one authored node's presentation for a runtime-populated row.
///
/// The source document remains the owner of geometry, typography, and visual
/// states. Shell code supplies only the localized row value and its domain
/// interaction components.
pub(crate) fn insert_authored_text_prototype(
    commands: &mut Commands,
    entity: Entity,
    document_root: Entity,
    document: &UiDocumentAsset,
    prototype_index: u32,
    localization: crate::assets::localization::loaded_localization_queries::LoadedLocalizationView<
        '_,
    >,
    text: String,
) -> bool {
    let data = document.canonical_ui_document();
    let Some(record) = data.nodes.get(prototype_index as usize) else {
        return false;
    };
    let flags = record.flags.0;
    let style = &record.style;
    let text_color_style = if flags & UiNodeFlags::USE_LIST_BOX_COLOR.0 != 0 {
        authored_list_ancestor(document, prototype_index).map_or(style, |ancestor| &ancestor.style)
    } else {
        style
    };
    let mut node = project_authored_node_and_style_to_bevy_layout(record, style, flags);
    flow_authored_row(&mut node, Some(FlexDirection::Column), 0, 0);
    // The source list refresh repositions its children but retains each
    // child's authored extent. Runtime-populated map rows borrow the same
    // `locationbutton` prototype as the authored biome rows, including its
    // 20-pixel hit and row height.
    let row_height = authored_px(node.height);
    commands.entity(entity).insert(node);
    if row_height > 0.0 {
        commands
            .entity(entity)
            .insert(UiPhysicalListRowHeight::from_physical_pixels(row_height));
    }
    let cursor = AssetId(record.cursor.0);
    if cursor != AssetId::default() {
        commands.entity(entity).insert(UiCursorStyle(cursor));
    }
    project_authored_node_visuals_into_bevy_presentation(
        commands,
        entity,
        data,
        AssetId(record.id.0),
        &record.bindings,
        &record.visuals,
        &record.text,
        AssetId(record.text_format.0),
        style,
        text_color_style,
        document,
        localization,
        flags & UiNodeFlags::ENABLED.0 != 0,
        None,
        Some(text),
    );
    project_widget(
        commands,
        entity,
        document_root,
        document,
        record,
        prototype_index,
        localization,
    );
    true
}

/// Finds the first authored toggle row beneath any matching list node.
/// Inherited panel templates contribute scroll decorations before list rows,
/// so source child zero is not necessarily a reusable row prototype.
pub(crate) fn authored_first_toggle_list_row_index(
    document: &UiDocumentAsset,
    source: UiWidgetLiveCollectionSource,
) -> Option<u32> {
    let data = document.canonical_ui_document();
    let parent = data.nodes.iter().position(|record| {
        matches!(
            &record.widget,
            UiWidgetRecord::List {
                source: authored,
                ..
            } | UiWidgetRecord::ToggleSet {
                source: authored,
                ..
            } if *authored == source
        )
    })?;
    data.nodes[parent].children.iter().copied().find(|&index| {
        data.nodes
            .get(index as usize)
            .is_some_and(|record| matches!(record.kind, UiNodeKind::Toggle))
    })
}

/// Converts an authored row to flex layout, preserving its margins and dimensions.
pub(crate) fn flow_authored_row(
    node: &mut Node,
    flow: Option<FlexDirection>,
    authored_column_width: i32,
    authored_row_height: i32,
) {
    let margin = |value| match value {
        Val::Auto => px(0.0),
        value => value,
    };
    node.position_type = PositionType::Relative;
    let left = std::mem::replace(&mut node.left, Val::Auto);
    let right = std::mem::replace(&mut node.right, Val::Auto);
    let top = std::mem::replace(&mut node.top, Val::Auto);
    let bottom = std::mem::replace(&mut node.bottom, Val::Auto);
    // A list/grid owns advancement on its packing axis. Authored coordinates
    // on that axis are not per-row margins: the source list refresh discards
    // them when placing each child. The cross-axis coordinate remains the
    // authored inset within the row.
    if !matches!(
        flow,
        Some(FlexDirection::Column | FlexDirection::ColumnReverse)
    ) {
        node.margin.top = margin(top);
        node.margin.bottom = margin(bottom);
    }
    if !matches!(flow, Some(FlexDirection::Row | FlexDirection::RowReverse)) {
        node.margin.left = margin(left);
        node.margin.right = margin(right);
    }
    // An unbounded Blue Fang grid advances by its authored cell extent even
    // when a child is wider or taller than that cell. Preserve the child's
    // visible extent and express the overlap as an ordinary negative trailing
    // flex margin. Construction tabs, for example, are 64 pixels wide in
    // 47-pixel cells.
    match flow {
        Some(FlexDirection::Row | FlexDirection::RowReverse) if authored_column_width > 0 => {
            node.margin.right = px(authored_column_width as f32 - authored_px(node.width));
        }
        Some(FlexDirection::Column | FlexDirection::ColumnReverse) if authored_row_height > 0 => {
            node.margin.bottom = px(authored_row_height as f32 - authored_px(node.height));
        }
        _ => {}
    }
    node.align_self = AlignSelf::Stretch;
    node.flex_shrink = 0.0;
}

fn authored_px(value: Val) -> f32 {
    match value {
        Val::Px(value) => value.max(0.0),
        _ => 0.0,
    }
}

#[derive(Clone, Copy)]
struct AuthoredParentLayoutOwnership {
    flow: Option<FlexDirection>,
    column_width: i32,
    row_height: i32,
}

fn parent_layout_ownership(
    data: &UiDocument,
    record: &UiNodeDefinition,
) -> Option<AuthoredParentLayoutOwnership> {
    let parent = record.parent as usize;
    data.nodes.get(parent).and_then(|parent| {
        let grid = match &parent.widget {
            UiWidgetRecord::Grid(grid)
            | UiWidgetRecord::List { grid, .. }
            | UiWidgetRecord::TypeList { grid, .. }
            | UiWidgetRecord::AdoptionList { grid, .. }
            | UiWidgetRecord::FinanceList { grid, .. }
            | UiWidgetRecord::CatalogueDetails { grid } => grid,
            UiWidgetRecord::ToggleSet { grid, .. } | UiWidgetRecord::TreeElement { grid, .. } => {
                grid
            }
            _ => return None,
        };
        authored_grid_record_owns_child_layout(grid).then(|| AuthoredParentLayoutOwnership {
            flow: authored_unbounded_grid_packing_direction(grid.columns, grid.rows),
            column_width: grid.column_width,
            row_height: grid.row_height,
        })
    })
}

fn scroll_decoration(data: &UiDocument, index: usize) -> Option<(UiSliderAxis, usize)> {
    let record = data.nodes.get(index)?;
    let decoration = AssetId(record.id.0);
    let named_axis = (!record.name.is_empty())
        .then_some(record.name.as_str())
        .and_then(|name| {
            if name.eq_ignore_ascii_case("hscroll") {
                Some(UiSliderAxis::Horizontal)
            } else if name.eq_ignore_ascii_case("vscroll") {
                Some(UiSliderAxis::Vertical)
            } else {
                None
            }
        });
    let mut parent = data.nodes.get(index)?.parent as usize;
    while parent < index {
        let node = data.nodes.get(parent)?;
        if let UiWidgetRecord::Window {
            horizontal_scroll,
            vertical_scroll,
            ..
        } = &node.widget
        {
            if decoration == AssetId(horizontal_scroll.0) {
                return Some((UiSliderAxis::Horizontal, parent));
            }
            if decoration == AssetId(vertical_scroll.0) {
                return Some((UiSliderAxis::Vertical, parent));
            }
        }
        if let Some(axis) = named_axis {
            if matches!(
                &node.widget,
                UiWidgetRecord::List { .. }
                    | UiWidgetRecord::TypeList { .. }
                    | UiWidgetRecord::AdoptionList { .. }
                    | UiWidgetRecord::FinanceList { .. }
                    | UiWidgetRecord::ToggleSet { .. }
                    | UiWidgetRecord::TreeElement { .. }
            ) {
                return Some((axis, parent));
            }
        }
        let next = node.parent as usize;
        if next == parent {
            break;
        }
        parent = next;
    }
    None
}
