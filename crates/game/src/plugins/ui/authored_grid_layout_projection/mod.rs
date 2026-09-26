use bevy::prelude::*;
use openzt2_game_data::ui_document::widget_control::UiGridRecord;

use super::authored_ui_layout_participation::UiAuthoredLayoutDisplay;

pub(super) fn apply_authored_grid_record_to_bevy_node_layout(
    commands: &mut Commands,
    entity: Entity,
    grid: &UiGridRecord,
) {
    let authored_columns = grid.columns;
    let authored_rows = grid.rows;
    let columns = authored_columns.max(0) as u16;
    let rows = authored_rows.max(0) as u16;
    let column_width = grid.column_width.max(0) as f32;
    let row_height = grid.row_height.max(0) as f32;
    let column_gap = grid.x_spacing as f32;
    let row_gap = grid.y_spacing as f32;
    let initial_x = grid.initial_x;
    let initial_y = grid.initial_y;
    let auto_size = grid.auto_size;
    let display =
        if authored_unbounded_grid_packing_direction(authored_columns, authored_rows).is_some() {
            Display::Flex
        } else {
            Display::Grid
        };

    commands
        .entity(entity)
        .entry::<Node>()
        .and_modify(move |mut node| {
            let flow = authored_unbounded_grid_packing_direction(authored_columns, authored_rows);
            node.display = display;
            if let Some(direction) = flow {
                node.flex_direction = direction;
                node.flex_wrap = FlexWrap::NoWrap;
                node.justify_content = JustifyContent::Start;
                node.align_content = AlignContent::Start;
                node.align_items = AlignItems::Start;
                node.column_gap = px(column_gap);
                node.row_gap = px(row_gap);
                // An unbounded source grid with no authored extent on its
                // packing axis is content-sized. Tree-row fragments use
                // `h=0, rows=-1` for exactly this contract. Fixed-size list
                // viewports also use `rows=-1`, so retain a non-zero authored
                // extent rather than turning the viewport into its contents.
                match direction {
                    FlexDirection::Column | FlexDirection::ColumnReverse
                        if node.height == px(0.0) =>
                    {
                        node.height = Val::Auto;
                    }
                    FlexDirection::Row | FlexDirection::RowReverse if node.width == px(0.0) => {
                        node.width = Val::Auto;
                    }
                    _ => {}
                }
            }
            node.grid_template_columns = match (columns.max(1), column_width > 0.0) {
                (count, true) => RepeatedGridTrack::px(count, column_width),
                (count, false) => RepeatedGridTrack::auto(count),
            };
            node.grid_template_rows = match (rows.max(1), row_height > 0.0) {
                (count, true) => RepeatedGridTrack::px(count, row_height),
                (count, false) => RepeatedGridTrack::auto(count),
            };
            node.grid_auto_flow = if columns == 0 && rows > 0 {
                GridAutoFlow::Column
            } else {
                GridAutoFlow::Row
            };
            node.grid_auto_columns = vec![if column_width > 0.0 {
                GridTrack::px(column_width)
            } else {
                GridTrack::auto()
            }];
            node.grid_auto_rows = vec![if row_height > 0.0 {
                GridTrack::px(row_height)
            } else {
                GridTrack::auto()
            }];
            node.justify_content = JustifyContent::Start;
            node.align_content = AlignContent::Start;
            node.justify_items = JustifyItems::Start;
            node.align_items = AlignItems::Start;
            node.column_gap = px(column_gap);
            node.row_gap = px(row_gap);

            // The original initial coordinates offset the first cell, rather
            // than every child's authored region. Padding expresses that
            // container-owned inset without a custom child-placement pass.
            node.padding.left =
                add_authored_pixel_offset_to_layout_value(node.padding.left, initial_x);
            node.padding.top =
                add_authored_pixel_offset_to_layout_value(node.padding.top, initial_y);

            // Intrinsic Bevy sizing is the native equivalent of the legacy
            // post-layout receiver resize. Parent autosizing remains ordinary
            // flex/grid propagation and needs no imperative parent mutation.
            if auto_size {
                node.width = Val::Auto;
                node.height = Val::Auto;
            }
        });
    commands
        .entity(entity)
        .entry::<UiAuthoredLayoutDisplay>()
        .and_modify(move |mut authored| authored.set_authored_display(display));
}

/// Enables Bevy's canonical scroll axis for a list's authored packing direction.
///
/// `clipchildren` describes the non-scrolling axis. A list additionally owns a
/// `ScrollPosition`, so leaving both axes at `OverflowAxis::Clip` makes wheel
/// and scrollbar updates observable in state but inert in layout.
pub(super) fn apply_authored_list_scroll_axis_to_bevy_node_overflow(
    commands: &mut Commands,
    entity: Entity,
    grid: &UiGridRecord,
) {
    let horizontal = matches!(
        authored_unbounded_grid_packing_direction(grid.columns, grid.rows),
        Some(FlexDirection::Row)
    ) || (grid.columns == 0 && grid.rows > 0);
    commands
        .entity(entity)
        .entry::<Node>()
        .and_modify(move |mut node| {
            node.overflow = if horizontal {
                Overflow {
                    x: OverflowAxis::Scroll,
                    y: OverflowAxis::Clip,
                }
            } else {
                Overflow {
                    x: OverflowAxis::Clip,
                    y: OverflowAxis::Scroll,
                }
            };
        });
}

/// Whether authored children must participate in their parent's native layout.
///
/// A zero/zero toggle set is only an interaction group: its children retain
/// their authored absolute positions. Any declared row or column count makes
/// the set a layout owner, including the original format's `-1` unbounded-axis
/// sentinel.
pub(super) fn authored_grid_record_owns_child_layout(grid: &UiGridRecord) -> bool {
    grid.columns != 0 || grid.rows != 0
}

pub(super) fn authored_unbounded_grid_packing_direction(
    columns: i32,
    rows: i32,
) -> Option<FlexDirection> {
    match (columns, rows) {
        (-1, 0 | 1) => Some(FlexDirection::Row),
        (0 | 1, -1) => Some(FlexDirection::Column),
        _ => None,
    }
}

fn add_authored_pixel_offset_to_layout_value(value: Val, offset: i32) -> Val {
    match value {
        Val::Px(current) => px(current + offset as f32),
        value if offset == 0 => value,
        _ => px(offset as f32),
    }
}
