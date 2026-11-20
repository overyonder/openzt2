use bevy::{
    asset::RenderAssetUsages,
    ecs::relationship::RelationshipTarget,
    prelude::*,
    render::render_resource::{Extent3d, TextureDimension, TextureFormat},
};
use openzt2_game_data::{ui_document::widget::UiWidgetRecord, AssetId};

use openzt2_game_assets::ui_document::ui_document_asset_types_and_borrowing_queries::UiDocumentAsset;
use openzt2_interface_state::ui::authored_reusable_list_and_table_runtime_types::UiTablePolicy;
use openzt2_interface_state::ui::authored_ui_node_projection_components::UiDocumentRoot;
use openzt2_interface_state::ui::authored_ui_node_projection_components::UiNodeId;
use openzt2_interface_state::ui::authored_ui_node_projection_components::{
    UiDocumentMembers, UiDocumentOwner,
};

use openzt2_interface_state::information::overview::overview_types::OverviewMapCanvas;

use super::map_rasterization_operations::{
    blank_overview_map_pixels, is_terrain_or_water, OverviewTerrainWaterRaster,
    PendingOverviewMapPixels, OVERVIEW_MAP_IMAGE_EXTENT,
};

pub(crate) fn create_missing_authored_overview_map_canvas_entities(
    overview_map_surfaces: Query<
        (Entity, &UiNodeId, &UiDocumentOwner),
        (With<UiTablePolicy>, Without<OverviewMapCanvas>),
    >,
    ui_document_roots: Query<(&UiDocumentRoot, &UiDocumentMembers)>,
    projected_ui_nodes: Query<&UiNodeId>,
    children: Query<&Children>,
    existing_overview_map_canvases: Query<&OverviewMapCanvas>,
    ui_documents: Res<Assets<UiDocumentAsset>>,
    mut images: ResMut<Assets<Image>>,
    mut commands: Commands,
) {
    for (overview_map_surface, surface_node, document_owner) in &overview_map_surfaces {
        let Ok((document_root, members)) = ui_document_roots.get(document_owner.0) else {
            continue;
        };
        let Some(ui_document) = ui_documents.get(&document_root.document) else {
            continue;
        };
        let Some(UiWidgetRecord::WorldMap { layers, colors }) = ui_document
            .canonical_ui_document()
            .nodes
            .get(surface_node.index as usize)
            .map(|node_record| &node_record.widget)
        else {
            continue;
        };
        for (layer_index, layer_record) in layers.iter().enumerate() {
            let layer_index = layer_index as u32;
            let Some(source) = layer_record.canvas else {
                continue;
            };
            let authored_parent_node_id = AssetId(layer_record.node.0);
            let Some(projected_parent) = members.iter().find(|&entity| {
                projected_ui_nodes
                    .get(entity)
                    .is_ok_and(|node| node.id == authored_parent_node_id)
            }) else {
                continue;
            };
            if children.get(projected_parent).is_ok_and(|children| {
                children.iter().any(|entity| {
                    existing_overview_map_canvases
                        .get(entity)
                        .is_ok_and(|canvas| {
                            canvas.surface == overview_map_surface && canvas.layer == layer_index
                        })
                })
            }) {
                continue;
            }
            let overview_map_image = create_empty_overview_map_image(&mut images);
            let mut canvas = commands.spawn((
                Name::new("overview map authored layer"),
                OverviewMapCanvas {
                    surface: overview_map_surface,
                    layer: layer_index,
                    source,
                    palette: *colors,
                    painted: false,
                },
                PendingOverviewMapPixels::default(),
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
            if is_terrain_or_water(source) {
                canvas.insert(OverviewTerrainWaterRaster);
            }
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
    overview_map_image.data = Some(blank_overview_map_pixels());
    images.add(overview_map_image)
}
