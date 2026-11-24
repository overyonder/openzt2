use bevy::{ecs::entity::EntityHashSet, prelude::*};
use openzt2_interface_state::immersive_modes::immersive_mode_state_types::{
    ActiveImmersiveMode, ImmersiveHudSuppression, ImmersiveMode,
};
use openzt2_interface_state::ui::authored_ui_node_projection_components::UiEdgePanSurface;

#[derive(Component)]
pub(super) struct UiOverheadOnly;

use super::authored_tree_expansion_and_row_indentation::{
    UiAuthoredTreeExpansion, UiAuthoredTreeHeader, UiAuthoredTreeItem,
};

/// The authored layout mode restored when a source UI node is shown.
///
/// Closed windows use `Display::None`, rather than merely suppressing their
/// draw visibility, so Bevy and Taffy do not lay out every inactive UI
/// document each frame.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct UiAuthoredLayoutDisplay(Display);

impl UiAuthoredLayoutDisplay {
    pub fn from_authored_display(authored_display: Display) -> Self {
        Self(authored_display)
    }

    pub fn authored_display(&self) -> Display {
        self.0
    }

    pub(super) fn set_authored_display(&mut self, authored_display: Display) {
        self.0 = authored_display;
    }
}

#[allow(clippy::type_complexity)]
pub(super) fn project_authored_layout_participation(
    mut nodes: Query<(
        &Visibility,
        &UiAuthoredLayoutDisplay,
        Option<&ChildOf>,
        Has<UiAuthoredTreeHeader>,
        Has<UiOverheadOnly>,
        Option<&UiEdgePanSurface>,
        &mut Node,
    )>,
    changed_nodes: Query<
        Entity,
        (
            With<UiAuthoredLayoutDisplay>,
            Or<(
                Changed<Visibility>,
                Changed<UiAuthoredLayoutDisplay>,
                Changed<ChildOf>,
                Added<Node>,
                Added<UiOverheadOnly>,
                Added<UiAuthoredTreeHeader>,
                Changed<UiEdgePanSurface>,
            )>,
        ),
    >,
    mut detached: RemovedComponents<ChildOf>,
    changed_trees: Query<&Children, (With<UiAuthoredTreeItem>, Changed<UiAuthoredTreeExpansion>)>,
    mut removed_trees: RemovedComponents<UiAuthoredTreeExpansion>,
    children: Query<&Children>,
    trees: Query<&UiAuthoredTreeExpansion, With<UiAuthoredTreeItem>>,
    overhead_nodes: Query<Entity, With<UiOverheadOnly>>,
    suppression: Query<(), With<ImmersiveHudSuppression>>,
    changed_suppression: Query<
        (),
        Or<(Added<ImmersiveHudSuppression>, Changed<ActiveImmersiveMode>)>,
    >,
    modes: Query<&ActiveImmersiveMode>,
    mut removed_suppression: RemovedComponents<ImmersiveHudSuppression>,
    mut removed_modes: RemovedComponents<ActiveImmersiveMode>,
) {
    let mut dirty = EntityHashSet::default();
    dirty.extend(changed_nodes.iter());
    dirty.extend(detached.read().filter(|entity| nodes.contains(*entity)));
    for children in &changed_trees {
        dirty.extend(children.iter());
    }
    for tree in removed_trees.read() {
        if let Ok(children) = children.get(tree) {
            dirty.extend(children.iter());
        }
    }
    if !changed_suppression.is_empty()
        || !removed_suppression.is_empty()
        || !removed_modes.is_empty()
    {
        dirty.extend(overhead_nodes.iter());
    }
    removed_suppression.clear();
    removed_modes.clear();
    if dirty.is_empty() {
        return;
    }
    let suppress_hud = !suppression.is_empty()
        || modes.iter().any(|active| {
            matches!(
                active.mode,
                ImmersiveMode::FirstPerson | ImmersiveMode::SuperStaff
            )
        });
    for entity in dirty {
        let Ok((visibility, authored, parent, header, overhead_only, edge_pan, node)) =
            nodes.get_mut(entity)
        else {
            continue;
        };
        let collapsed = !header
            && parent.is_some_and(|parent| {
                trees
                    .get(parent.parent())
                    .is_ok_and(|expanded| !expanded.is_expanded())
            });
        let display = if *visibility == Visibility::Hidden
            || collapsed
            || overhead_only && suppress_hud
            || edge_pan.is_some_and(|surface| !surface.0)
        {
            Display::None
        } else {
            authored.authored_display()
        };
        node.map_unchanged(|node| &mut node.display)
            .set_if_neq(display);
    }
}
