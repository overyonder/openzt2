use bevy::prelude::*;
use openzt2_game_data::ui_document::node_layout::UiNodeRegionMetric;
use openzt2_game_data::ui_document::widget::UiWidgetRecord;

use openzt2_game_assets::ui_document::ui_document_asset_types_and_borrowing_queries::UiDocumentAsset;

use super::authored_ui_node_projection_components::{
    UiDocumentOwner, UiDocumentRoot, UiNodeId, UiValue,
};
use super::authored_ui_visual_types::UiSourceRect;

/// Identifies the node whose immutable multi-icon entries remain in the
/// owning UI document. `UiValue` is the only live selection state.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct UiMultiIconPolicy;

/// Displays the document image selected by `UiValue`.
pub(crate) fn present_multi_icons(
    mut commands: Commands,
    documents: Res<Assets<UiDocumentAsset>>,
    roots: Query<&UiDocumentRoot>,
    mut icons: Query<
        (
            Entity,
            &UiNodeId,
            &UiDocumentOwner,
            &UiMultiIconPolicy,
            &UiValue,
            &mut UiSourceRect,
            Option<&mut ImageNode>,
            &mut Visibility,
        ),
        Changed<UiValue>,
    >,
) {
    for (entity, node, owner, _, value, mut source, image_node, mut visibility) in &mut icons {
        let Some(index) = usize::try_from(value.0).ok() else {
            *visibility = Visibility::Hidden;
            continue;
        };
        let Ok(root) = roots.get(owner.0) else {
            continue;
        };
        let Some(document) = documents.get(&root.document) else {
            continue;
        };
        let Some(entry) = document
            .canonical_ui_document()
            .nodes
            .get(node.index as usize)
            .and_then(|node| match &node.widget {
                UiWidgetRecord::MultiIcon { entries } => entries.get(index),
                _ => None,
            })
        else {
            *visibility = Visibility::Hidden;
            continue;
        };
        let Some(image) =
            document.cloned_texture_image_handle(openzt2_game_data::AssetId(entry.image.0))
        else {
            *visibility = Visibility::Hidden;
            continue;
        };
        let rect = {
            let pixel = |metric: &UiNodeRegionMetric| match metric {
                UiNodeRegionMetric::Pixels(value) => Some(*value),
                UiNodeRegionMetric::OutsideTop => None,
            };
            let x = pixel(&entry.source_rect.metrics[0]);
            let y = pixel(&entry.source_rect.metrics[1]);
            let width = pixel(&entry.source_rect.metrics[2]);
            let height = pixel(&entry.source_rect.metrics[3]);
            x.zip(y)
                .zip(width.zip(height))
                .and_then(|((x, y), (width, height))| {
                    (width > 0.0 && height > 0.0).then_some(Rect {
                        min: Vec2::new(x, y),
                        max: Vec2::new(x + width, y + height),
                    })
                })
        };
        source.set_if_neq(UiSourceRect(rect.map_or([0, 0, -1, -1], |rect| {
            [
                rect.min.x as i32,
                rect.min.y as i32,
                rect.width() as i32,
                rect.height() as i32,
            ]
        })));
        if let Some(image_node) = image_node {
            image_node
                .map_unchanged(|node| &mut node.image)
                .set_if_neq(image);
        } else {
            commands.entity(entity).insert(ImageNode {
                image,
                rect,
                ..default()
            });
        }
        *visibility = Visibility::Inherited;
    }
}
