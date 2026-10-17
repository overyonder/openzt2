//! Ground cursor shown by the original fence mode in place of its blank pointer.

use std::path::Path;

use bevy::{camera::visibility::RenderLayers, prelude::*};

use crate::plugins::construction::construction_interaction_types::ConstructionCursor;
use crate::plugins::construction::construction_tool_and_placement_policy_types::ConstructionTool;
use crate::plugins::terrain::terrain_chunk_presentation_types::TERRAIN_RENDER_LAYER;
use crate::plugins::terrain::terrain_chunk_types::{
    EditedTerrainSamples, TerrainChunk, TerrainIndex,
};
use crate::plugins::terrain::terrain_fitted_surface_mesh_construction::construct_square_surface_mesh_fitted_to_canonical_terrain_samples;
use crate::plugins::terrain::terrain_world_sampling::{sample_terrain, terrain_chunk_at};
use crate::plugins::world_spawn::prefab_presentation_types::PrefabPresentation;
use crate::plugins::world_spawn::world_membership_types::{WorldMember, WorldRoot};
use openzt2_game_assets::asset_source::AssetArchives;
use openzt2_game_assets::model_source::native_model_source_lowering::native_model_scene_labelled_asset_path;
use openzt2_game_assets::terrain::terrain_asset_types_and_borrowing_queries::TerrainAsset;

use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use openzt2_game_assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;

use super::{
    topology_construction_planning::fence_definition_is_gate, topology_graph_types::TopologyGrid,
};

// ponytail: `ZTFenceMode` names `anchor="ui/fx/anchor.xml"` and
// `grid="ui/fx/fencegrid.xml"`; their model and decal are fixed here like the
// terrain brush cursors. Lower them through the UI document if a mod replaces them.
const ANCHOR_DOCUMENT: &str = "ui/fx/anchor.xml";
const ANCHOR_MODEL: &str = "ui/fx/anchor/anchor.nif";
const GRID_IMAGE: &str = "ui/fx/footprint/diag_card_grid_small.tga";
const GRID_WIDTH_M: f32 = 22.5885;

#[derive(Component)]
pub(super) struct FenceAnchorCursor {
    cell: Option<IVec3>,
}

/// Places the authored anchor post and fence grid on the grid point under the
/// pointer while a fence is selected; the mode's own pointer is `blank.cur`.
#[allow(clippy::too_many_arguments)]
pub(super) fn present_fence_anchor_cursor(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    archives: Res<AssetArchives>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    tool: Res<ConstructionTool>,
    active_definitions: Res<WorldDefinitions>,
    definitions: Res<Assets<WorldDefinitionAsset>>,
    grid: Res<TopologyGrid>,
    ui_capture: Res<crate::plugins::ui::picking::UiPointerCapture>,
    terrain_index: Res<TerrainIndex>,
    terrain_assets: Res<Assets<TerrainAsset>>,
    terrain_chunks: Query<(&TerrainChunk, Option<&EditedTerrainSamples>)>,
    pointer: Query<&ConstructionCursor>,
    roots: Query<Entity, With<WorldRoot>>,
    mut anchors: Query<(
        Entity,
        &mut FenceAnchorCursor,
        &mut Transform,
        &mut Visibility,
        Option<&Mesh3d>,
    )>,
) {
    // A gate goes onto an existing fence, so it has no anchor post to show.
    let gate = matches!(*tool, ConstructionTool::Fence(definition)
        if active_definitions.get(&definitions)
            .is_some_and(|definitions| fence_definition_is_gate(definitions, definition)));
    let (Ok(pointer), Ok(root), ConstructionTool::Fence(_), false) =
        (pointer.single(), roots.single(), *tool, gate)
    else {
        for (entity, ..) in &anchors {
            commands.entity(entity).despawn();
        }
        return;
    };
    let Ok((entity, mut anchor, mut transform, mut visibility, mesh)) = anchors.single_mut() else {
        let entity = commands
            .spawn((
                Name::new("fence anchor cursor"),
                FenceAnchorCursor { cell: None },
                Transform::default(),
                Visibility::Hidden,
                WorldMember { root },
            ))
            .id();
        // Expansions replace the anchor with a Blue Fang model, as the
        // planner does for authored `BFSceneGraphComponent` models.
        if let Some(model) = archives.resolve_model_reference_with_blue_fang_bfb_preference(
            Path::new(ANCHOR_DOCUMENT),
            ANCHOR_MODEL,
            true,
        ) {
            let model = model.to_string_lossy().replace('\\', "/");
            commands.spawn((
                Transform::default(),
                Visibility::Inherited,
                ChildOf(entity),
                PrefabPresentation::new(
                    asset_server.load(native_model_scene_labelled_asset_path(&model)),
                ),
            ));
        }
        return;
    };
    visibility.set_if_neq(if ui_capture.over_ui {
        Visibility::Hidden
    } else {
        Visibility::Inherited
    });
    let mut cell = grid.position_cell(pointer.world);
    cell.z = 0;
    if anchor.cell == Some(cell) {
        return;
    }
    let mut centre = grid.cell_translation(cell);
    let Some(height) = terrain_chunk_at(&terrain_index, centre.xz())
        .and_then(|chunk| terrain_chunks.get(chunk).ok())
        .and_then(|(chunk, edited)| {
            sample_terrain(
                chunk,
                terrain_assets.get(&chunk.asset)?,
                edited,
                centre.xz(),
            )
        })
        .map(|point| point.height_m)
    else {
        return;
    };
    centre.y = height;
    transform.translation = centre;
    anchor.cell = Some(cell);
    let Some(decal) = construct_square_surface_mesh_fitted_to_canonical_terrain_samples(
        GRID_WIDTH_M,
        centre,
        &terrain_index,
        &terrain_assets,
        &terrain_chunks,
    ) else {
        return;
    };
    match mesh {
        Some(mesh) => {
            if let Some(mut mesh) = meshes.get_mut(&mesh.0) {
                *mesh = decal;
            }
        }
        None => {
            commands.entity(entity).insert((
                Mesh3d(meshes.add(decal)),
                MeshMaterial3d(materials.add(StandardMaterial {
                    base_color_texture: Some(asset_server.load(GRID_IMAGE)),
                    alpha_mode: AlphaMode::Blend,
                    depth_bias: 1.0,
                    unlit: true,
                    ..default()
                })),
                RenderLayers::layer(TERRAIN_RENDER_LAYER),
            ));
        }
    }
}
