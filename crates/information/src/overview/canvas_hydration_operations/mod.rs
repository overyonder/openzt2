use bevy::{
    asset::RenderAssetUsages,
    ecs::relationship::RelationshipTarget,
    prelude::*,
    render::render_resource::{Extent3d, TextureDimension, TextureFormat},
};
use openzt2_game_data::ui_document::{
    overview_map_presentation::{UiMapColorsRecord, UiOverviewCanvas},
    widget::UiWidgetRecord,
};

use openzt2_game_assets::ui_document::ui_document_asset_types_and_borrowing_queries::UiDocumentAsset;
use openzt2_interface_state::ui::{
    authored_reusable_list_and_table_runtime_types::UiTablePolicy,
    authored_ui_node_projection_components::{
        UiDocumentMembers, UiDocumentOwner, UiDocumentRoot, UiNodeId,
    },
};

use super::map_rasterization_operations::{
    OVERVIEW_MAP_IMAGE_EXTENT, OverviewTerrainWaterRaster, PendingOverviewMapPixels,
    blank_overview_map_pixels, is_terrain_or_water,
};

#[derive(Component)]
#[relationship(relationship_target = OverviewMapSurfaceLayers)]
pub(crate) struct OverviewMapLayerOf {
    #[relationship]
    surface: Entity,
    index: u32,
}

#[derive(Component)]
#[relationship_target(relationship = OverviewMapLayerOf, linked_spawn)]
pub(crate) struct OverviewMapSurfaceLayers(Vec<Entity>);

#[derive(Component, Clone, Copy, PartialEq, Eq)]
pub(crate) struct OverviewCanvasPolicy {
    pub(super) source: UiOverviewCanvas,
    pub(super) palette: UiMapColorsRecord,
}

pub(crate) fn create_missing_authored_overview_map_canvas_entities(
    surfaces: Query<
        (
            Entity,
            Ref<UiNodeId>,
            Ref<UiDocumentOwner>,
            Option<&OverviewMapSurfaceLayers>,
        ),
        With<UiTablePolicy>,
    >,
    roots: Query<(Ref<UiDocumentRoot>, Ref<UiDocumentMembers>)>,
    nodes: Query<&UiNodeId>,
    mut canvases: Query<(&OverviewMapLayerOf, &ChildOf, &mut OverviewCanvasPolicy)>,
    documents: Res<Assets<UiDocumentAsset>>,
    mut images: ResMut<Assets<Image>>,
    mut commands: Commands,
) {
    for (surface, node, document_owner, layers) in &surfaces {
        let Ok((root, members)) = roots.get(document_owner.0) else {
            continue;
        };
        if !documents.is_changed()
            && !root.is_changed()
            && !members.is_changed()
            && !node.is_changed()
            && !document_owner.is_changed()
        {
            continue;
        }
        let Some(document) = documents.get(&root.document) else {
            continue;
        };
        let mut existing = layers
            .into_iter()
            .flat_map(RelationshipTarget::iter)
            .filter_map(|entity| Some((canvases.get(entity).ok()?.0.index, entity)))
            .collect::<std::collections::BTreeMap<_, _>>();
        if let Some(UiWidgetRecord::WorldMap { layers, colors }) = document
            .canonical_ui_document()
            .nodes
            .get(node.index as usize)
            .map(|node| &node.widget)
        {
            for (index, layer) in layers.iter().enumerate() {
                let Some(source) = layer.canvas else {
                    continue;
                };
                let Some(parent) = members
                    .iter()
                    .find(|&entity| nodes.get(entity).is_ok_and(|node| node.id == layer.node))
                else {
                    continue;
                };
                let index = index as u32;
                let next = OverviewCanvasPolicy {
                    source,
                    palette: *colors,
                };
                if let Some(entity) = existing.remove(&index) {
                    if let Ok((_, old_parent, mut policy)) = canvases.get_mut(entity) {
                        if old_parent.parent() != parent {
                            commands.entity(entity).insert(ChildOf(parent));
                        }
                        if is_terrain_or_water(policy.source) != is_terrain_or_water(source) {
                            if is_terrain_or_water(source) {
                                commands.entity(entity).insert(OverviewTerrainWaterRaster);
                            } else {
                                commands
                                    .entity(entity)
                                    .try_remove::<OverviewTerrainWaterRaster>();
                            }
                        }
                        policy.set_if_neq(next);
                    }
                } else {
                    let mut canvas = commands.spawn((
                        Name::new("overview map authored layer"),
                        OverviewMapLayerOf { surface, index },
                        next,
                        PendingOverviewMapPixels::default(),
                        Visibility::Hidden,
                        Node {
                            position_type: PositionType::Absolute,
                            left: Val::Px(0.0),
                            top: Val::Px(0.0),
                            width: Val::Percent(100.0),
                            height: Val::Percent(100.0),
                            ..default()
                        },
                        ZIndex(-10 + i32::try_from(index).unwrap_or_default()),
                        ImageNode::new(create_empty_overview_map_image(&mut images)),
                        Pickable::IGNORE,
                        ChildOf(parent),
                    ));
                    if is_terrain_or_water(source) {
                        canvas.insert(OverviewTerrainWaterRaster);
                    }
                }
            }
        }
        for entity in existing.into_values() {
            commands.entity(entity).try_despawn();
        }
    }
}

fn create_empty_overview_map_image(images: &mut Assets<Image>) -> Handle<Image> {
    let mut image = Image::new_uninit(
        Extent3d {
            width: OVERVIEW_MAP_IMAGE_EXTENT,
            height: OVERVIEW_MAP_IMAGE_EXTENT,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
    );
    image.data = Some(blank_overview_map_pixels());
    images.add(image)
}
