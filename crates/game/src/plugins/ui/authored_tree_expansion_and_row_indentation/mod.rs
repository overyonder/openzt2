use bevy::prelude::*;
use openzt2_game_data::ui_document::action::UiTrigger;
use openzt2_game_data::AssetId;

use super::authored_reusable_list_and_table_runtime_types::UiListRow;
use super::authored_ui_interaction_enabled_state::UiInteractionEnabled;
use super::authored_ui_layout_participation::UiAuthoredLayoutDisplay;
use crate::plugins::ui::authored_ui_activation_contracts::UiNodeActivated;

// UITreeElement reserves one 22-pixel leading gutter for its expander and
// advances descendants by 13 pixels. These are control-layout semantics: the
// authored fragment's own x coordinate remains intact and domain projections
// supply only a zero-based depth.
const TREE_LEADING_GUTTER_PX: f32 = 22.0;
const TREE_LEVEL_INDENT_PX: f32 = 13.0;

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct UiAuthoredTreeItem {
    item: AssetId,
    /// Zero-based hierarchy level projected by the domain that owns the tree
    /// relation. This owner turns the primitive depth into the standard source
    /// control inset.
    depth: u16,
}

impl UiAuthoredTreeItem {
    pub(crate) fn from_authored_item_and_initial_depth(item: AssetId, depth: u16) -> Self {
        Self { item, depth }
    }

    pub(crate) fn authored_item(&self) -> AssetId {
        self.item
    }

    pub(crate) fn hierarchy_depth(&self) -> u16 {
        self.depth
    }

    pub(crate) fn set_authored_item(&mut self, item: AssetId) {
        self.item = item;
    }

    pub(crate) fn set_hierarchy_depth(&mut self, depth: u16) {
        self.depth = depth;
    }
}

#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct UiAuthoredTreeExpansion {
    expanded: bool,
}

impl UiAuthoredTreeExpansion {
    pub(crate) fn from_authored_expansion(expanded: bool) -> Self {
        Self { expanded }
    }

    pub(crate) fn is_expanded(&self) -> bool {
        self.expanded
    }

    pub(crate) fn set_expanded(&mut self, expanded: bool) {
        self.expanded = expanded;
    }
}

/// The authored header row that remains available while its tree element is
/// collapsed. Dynamic child rows are ordinary siblings beneath the same tree
/// element and leave Bevy layout while collapsed.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct UiAuthoredTreeHeader;

#[derive(Component, Clone, Copy)]
pub(super) struct UiAuthoredTreeRowPhysicalIndent {
    pixels: f32,
}

impl UiAuthoredTreeRowPhysicalIndent {
    pub(super) fn physical_pixels(&self) -> f32 {
        self.pixels
    }
}

pub(super) fn project_authored_tree_depth_into_containing_row_indentation(
    mut commands: Commands,
    elements: Query<(Entity, &UiAuthoredTreeItem), Changed<UiAuthoredTreeItem>>,
    parents: Query<&ChildOf>,
    rows: Query<(), With<UiListRow>>,
    mut nodes: Query<&mut Node>,
) {
    for (entity, item) in &elements {
        // The tree widget is the expander inside a reusable list-row. Source
        // indentation moves that row; moving the inner widget compounds its
        // authored icon/text spacing and leaves sibling content behind.
        let row = std::iter::successors(Some(entity), |entity| {
            parents.get(*entity).ok().map(ChildOf::parent)
        })
        .find(|entity| rows.contains(*entity));
        if let Some(row) = row {
            commands
                .entity(row)
                .insert(UiAuthoredTreeRowPhysicalIndent {
                    pixels: TREE_LEADING_GUTTER_PX
                        + f32::from(item.hierarchy_depth()) * TREE_LEVEL_INDENT_PX,
                });
        }
        if let Ok(mut node) = nodes.get_mut(entity) {
            node.margin.left = Val::ZERO;
        }
    }
}

/// Toggles one enabled tree element when it is pressed.
pub(super) fn toggle_authored_tree_expansion_from_presses(
    mut activated: MessageReader<UiNodeActivated>,
    mut elements: Query<
        (&UiInteractionEnabled, &mut UiAuthoredTreeExpansion),
        With<UiAuthoredTreeItem>,
    >,
) {
    for activation in activated.read() {
        if activation.trigger != UiTrigger::Press {
            continue;
        }
        let Ok((enabled, mut expanded)) = elements.get_mut(activation.node) else {
            continue;
        };
        if enabled.0 {
            let next_expansion = !expanded.is_expanded();
            expanded.set_expanded(next_expansion);
        }
    }
}

/// Includes or excludes authored tree children from Bevy layout.
///
/// Visual layers are not authored document nodes and therefore do not carry
/// `UiAuthoredLayoutDisplay`; they remain governed by the ordinary interaction
/// presentation system.
pub(super) fn present_authored_tree_children_from_expansion(
    elements: Query<(&Children, &UiAuthoredTreeExpansion), Changed<UiAuthoredTreeExpansion>>,
    mut nodes: Query<(
        &UiAuthoredLayoutDisplay,
        &Visibility,
        Has<UiAuthoredTreeHeader>,
        &mut Node,
    )>,
) {
    for (children, expanded) in &elements {
        for child in children.iter() {
            let Ok((authored, visibility, header, mut node)) = nodes.get_mut(child) else {
                continue;
            };
            let display = if *visibility != Visibility::Hidden && (expanded.is_expanded() || header)
            {
                authored.authored_display()
            } else {
                Display::None
            };
            if node.display != display {
                node.display = display;
            }
        }
    }
}
