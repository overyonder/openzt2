use bevy::{
    asset::RenderAssetUsages,
    prelude::*,
    render::render_resource::{Extent3d, TextureDimension, TextureFormat},
};
use openzt2_game_data::{ui_document::widget::UiWidgetRecord, AssetId};

use crate::{
    assets::ui_document::ui_document_asset_types_and_borrowing_queries::UiDocumentAsset,
    plugins::ui::{
        authored_reusable_list_and_table_runtime_types::UiTablePolicy,
        authored_ui_node_projection_components::UiDocumentOwner,
        authored_ui_node_projection_components::UiDocumentRoot,
        authored_ui_node_projection_components::UiNodeId,
    },
};

use super::overview_types::OverviewMapCanvas;

const OVERVIEW_MAP_IMAGE_EXTENT: u32 = 512;

pub(in crate::plugins::information) fn create_missing_authored_overview_map_canvas_entities(
    overview_map_surfaces: Query<
        (Entity, &UiNodeId, &UiDocumentOwner),
        (With<UiTablePolicy>, Without<OverviewMapCanvas>),
    >,
    ui_document_roots: Query<&UiDocumentRoot>,
    projected_ui_nodes: Query<(Entity, &UiNodeId, &UiDocumentOwner)>,
    existing_overview_map_canvases: Query<&OverviewMapCanvas>,
    ui_documents: Res<Assets<UiDocumentAsset>>,
    mut images: ResMut<Assets<Image>>,
    mut commands: Commands,
) {
    for (overview_map_surface, surface_node, document_owner) in &overview_map_surfaces {
        let Ok(document_root) = ui_document_roots.get(document_owner.0) else {
            continue;
        };
        let Some(ui_document) = ui_documents.get(&document_root.document) else {
            continue;
        };
        let Some(UiWidgetRecord::WorldMap { layers, .. }) = ui_document
            .canonical_ui_document()
            .nodes
            .get(surface_node.index as usize)
            .map(|node_record| &node_record.widget)
        else {
            continue;
        };
        for (layer_index, layer_record) in layers.iter().enumerate() {
            let layer_index = layer_index as u32;
            if existing_overview_map_canvases
                .iter()
                .any(|canvas| canvas.surface == overview_map_surface && canvas.layer == layer_index)
                || layer_record.canvas.is_none()
            {
                continue;
            }
            let authored_parent_node_id = AssetId(layer_record.node.0);
            let projected_parent = projected_ui_nodes
                .iter()
                .find_map(|(projected_node, candidate_node_id, candidate_owner)| {
                    (candidate_owner.0 == document_owner.0
                        && candidate_node_id.id == authored_parent_node_id)
                        .then_some(projected_node)
                })
                .unwrap_or(overview_map_surface);
            let overview_map_image = create_empty_overview_map_image(&mut images);
            commands.spawn((
                Name::new("overview map authored layer"),
                OverviewMapCanvas {
                    surface: overview_map_surface,
                    layer: layer_index,
                    painted: false,
                },
                Node {
                    position_type: PositionType::Absolute,
                    left: Val::Px(0.0),
                    top: Val::Px(0.0),
                    width: Val::Percent(100.0),
                    height: Val::Percent(100.0),
                    ..default()
                },
                ZIndex(-10 + i32::try_from(layer_index).unwrap_or_default()),
                ImageNode::new(overview_map_image),
                Pickable::IGNORE,
                ChildOf(projected_parent),
            ));
        }
    }
}

fn create_empty_overview_map_image(images: &mut Assets<Image>) -> Handle<Image> {
    let mut overview_map_image = Image::new_uninit(
        Extent3d {
            width: OVERVIEW_MAP_IMAGE_EXTENT,
            height: OVERVIEW_MAP_IMAGE_EXTENT,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
    );
    overview_map_image.data = Some(vec![
        0;
        OVERVIEW_MAP_IMAGE_EXTENT as usize
            * OVERVIEW_MAP_IMAGE_EXTENT as usize
            * 4
    ]);
    images.add(overview_map_image)
}
