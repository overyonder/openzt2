use taffy::prelude::*;
use taffy::{Direction, Overflow, Point};

#[test]
fn grid_scroll_extent_includes_all_tracks_and_container_padding() {
    for direction in [Direction::Ltr, Direction::Rtl] {
        for padding in [0.0, 7.0] {
            let mut tree = TaffyTree::<()>::new();
            let children: Vec<_> = (0..64)
                .map(|_| {
                    tree.new_leaf(Style {
                        size: Size {
                            width: length(57.0),
                            height: length(48.0),
                        },
                        ..Default::default()
                    })
                    .unwrap()
                })
                .collect();
            let root = tree
                .new_with_children(
                    Style {
                        display: Display::Grid,
                        direction,
                        size: Size {
                            width: length(400.0),
                            height: length(118.0),
                        },
                        padding: Rect::length(padding),
                        border: Rect::length(3.0),
                        grid_template_rows: vec![length(59.0), length(59.0)],
                        grid_auto_columns: vec![length(57.0)],
                        grid_auto_flow: GridAutoFlow::Column,
                        overflow: Point {
                            x: Overflow::Scroll,
                            y: Overflow::Clip,
                        },
                        justify_content: Some(JustifyContent::Start),
                        align_content: Some(AlignContent::Start),
                        ..Default::default()
                    },
                    &children,
                )
                .unwrap();
            tree.compute_layout(root, Size::MAX_CONTENT).unwrap();
            let layout = tree.layout(root).unwrap();
            // 32 columns of 57px, two rows with 59px pitch and 48px items.
            // Content includes both padding edges, but excludes the border.
            assert_eq!(layout.content_size.width, 32.0 * 57.0 + 2.0 * padding);
            assert_eq!(layout.content_size.height, 59.0 + 48.0 + 2.0 * padding);
        }
    }
}
