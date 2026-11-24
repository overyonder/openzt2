use bevy::prelude::*;
use openzt2_game_data::ui_document::action::UiTrigger;
use openzt2_game_data::AssetId;

use super::authored_reusable_list_and_table_runtime_types::UiListRow;
use super::authored_ui_interaction_enabled_state::UiInteractionEnabled;
use openzt2_interface_state::ui::authored_ui_activation_contracts::UiNodeActivated;

// UITreeElement reserves one 22-pixel leading gutter for its expander and
// advances descendants by 13 pixels. These are control-layout semantics: the
// authored fragment's own x coordinate remains intact and domain projections
// supply only a zero-based depth.
const TREE_LEADING_GUTTER_PX: f32 = 22.0;
const TREE_LEVEL_INDENT_PX: f32 = 13.0;

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct UiAuthoredTreeItem {
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

    pub fn authored_item(&self) -> AssetId {
        self.item
    }

    pub fn hierarchy_depth(&self) -> u16 {
        self.depth
    }

    pub fn set_authored_item(&mut self, item: AssetId) {
        self.item = item;
    }

    pub fn set_hierarchy_depth(&mut self, depth: u16) {
        self.depth = depth;
    }
}

#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct UiAuthoredTreeExpansion {
    expanded: bool,
}

impl UiAuthoredTreeExpansion {
    pub(crate) fn from_authored_expansion(expanded: bool) -> Self {
        Self { expanded }
    }

    pub fn is_expanded(&self) -> bool {
        self.expanded
    }

    pub fn set_expanded(&mut self, expanded: bool) {
        self.expanded = expanded;
    }
}

/// The authored header row that remains available while its tree element is
/// collapsed. Dynamic child rows are ordinary siblings beneath the same tree
/// element and leave Bevy layout while collapsed.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct UiAuthoredTreeHeader;

#[derive(Component, Clone, Copy, PartialEq)]
pub(super) struct UiAuthoredTreeRowPhysicalIndent {
    pixels: f32,
}

#[derive(Component, PartialEq, Eq)]
#[relationship(relationship_target = RowTreeItems)]
pub(super) struct TreeItemInUiRow(Entity);

#[derive(Component)]
#[relationship_target(relationship = TreeItemInUiRow)]
pub(super) struct RowTreeItems(Vec<Entity>);

pub(super) fn bind_tree_items_to_containing_rows(
    mut commands: Commands,
    items: Query<
        (Entity, Option<&TreeItemInUiRow>),
        (
            With<UiAuthoredTreeItem>,
            Or<(Added<UiAuthoredTreeItem>, Changed<ChildOf>)>,
        ),
    >,
    parents: Query<&ChildOf>,
    rows: Query<(), With<UiListRow>>,
) {
    for (item, binding) in &items {
        let row = std::iter::once(item)
            .chain(parents.iter_ancestors::<ChildOf>(item))
            .find(|entity| rows.contains(*entity));
        match (row, binding) {
            (Some(row), old) if old != Some(&TreeItemInUiRow(row)) => {
                commands.entity(item).insert(TreeItemInUiRow(row));
            }
            (None, Some(_)) => {
                commands.entity(item).remove::<TreeItemInUiRow>();
            }
            _ => {}
        }
    }
}

pub(super) fn update_tree_row_indentation(
    mut commands: Commands,
    items: Query<
        (Entity, &UiAuthoredTreeItem, Option<&TreeItemInUiRow>),
        Or<(Changed<UiAuthoredTreeItem>, Changed<TreeItemInUiRow>)>,
    >,
    mut row_indents: Query<Option<&mut UiAuthoredTreeRowPhysicalIndent>, With<UiListRow>>,
    mut nodes: Query<&mut Node>,
) {
    for (entity, item, row) in &items {
        if let Some(row) = row {
            let indent = UiAuthoredTreeRowPhysicalIndent {
                pixels: TREE_LEADING_GUTTER_PX
                    + f32::from(item.hierarchy_depth()) * TREE_LEVEL_INDENT_PX,
            };
            if let Ok(existing) = row_indents.get_mut(row.0) {
                if let Some(mut existing) = existing {
                    existing.set_if_neq(indent);
                } else {
                    commands.entity(row.0).insert(indent);
                }
            }
        }
        if let Ok(node) = nodes.get_mut(entity) {
            node.map_unchanged(|node| &mut node.margin.left)
                .set_if_neq(Val::ZERO);
        }
    }
}

impl UiAuthoredTreeRowPhysicalIndent {
    pub(super) fn physical_pixels(&self) -> f32 {
        self.pixels
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
