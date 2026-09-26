use bevy::prelude::*;

use super::authored_tree_expansion_and_row_indentation::{
    UiAuthoredTreeExpansion, UiAuthoredTreeHeader, UiAuthoredTreeItem,
};

/// The authored layout mode restored when a source UI node is shown.
///
/// Closed windows use `Display::None`, rather than merely suppressing their
/// draw visibility, so Bevy and Taffy do not lay out every inactive UI
/// document each frame.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct UiAuthoredLayoutDisplay(Display);

impl UiAuthoredLayoutDisplay {
    pub(crate) fn from_authored_display(authored_display: Display) -> Self {
        Self(authored_display)
    }

    pub(crate) fn authored_display(&self) -> Display {
        self.0
    }

    pub(super) fn set_authored_display(&mut self, authored_display: Display) {
        self.0 = authored_display;
    }
}

/// Keeps source show/hide semantics on Bevy's layout boundary.
///
/// `Visibility::Hidden` alone only suppresses rendering; Taffy would still
/// measure every closed source window and every descendant. The authored
/// display mode is interaction state for the projected node, while Bevy's
/// `Node` remains the sole live layout owner.
pub(super) fn synchronize_authored_visibility_with_bevy_layout_participation(
    mut nodes: Query<
        (
            &Visibility,
            &UiAuthoredLayoutDisplay,
            Option<&ChildOf>,
            Has<UiAuthoredTreeHeader>,
            &mut Node,
        ),
        Changed<Visibility>,
    >,
    trees: Query<&UiAuthoredTreeExpansion, With<UiAuthoredTreeItem>>,
) {
    for (visibility, authored, parent, header, mut node) in &mut nodes {
        let collapsed_tree_content = !header
            && parent.is_some_and(|parent| {
                trees
                    .get(parent.parent())
                    .is_ok_and(|expanded| !expanded.is_expanded())
            });
        let display = if matches!(visibility, Visibility::Hidden) || collapsed_tree_content {
            Display::None
        } else {
            authored.authored_display()
        };
        if node.display != display {
            node.display = display;
        }
    }
}
