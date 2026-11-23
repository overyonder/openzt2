//! Ground cursor shown by the original fence mode in place of its blank pointer.

use std::path::Path;

use bevy::{camera::visibility::RenderLayers, prelude::*};

use openzt2_game_assets::asset_source::AssetArchives;
use openzt2_game_assets::model_source::native_model_source_lowering::native_model_scene_labelled_asset_path;
use openzt2_game_assets::terrain::terrain_asset_types_and_borrowing_queries::TerrainAsset;
use openzt2_world_state::world_spawn::prefab_presentation_types::PrefabPresentation;
use openzt2_world_state::world_spawn::world_membership_types::{WorldMember, WorldRoot};
use openzt2_zoo_state::construction::construction_interaction_types::ConstructionCursor;
use openzt2_zoo_state::construction::construction_tool_and_placement_policy_types::ConstructionTool;
use openzt2_zoo_state::terrain::terrain_chunk_presentation_types::TERRAIN_RENDER_LAYER;
use openzt2_zoo_state::terrain::terrain_chunk_types::{
    EditedTerrainSamples, TerrainChunk, TerrainIndex,
};
use openzt2_zoo_state::terrain::terrain_edit_types::TerrainChanged;
use openzt2_zoo_state::terrain::terrain_fitted_surface_mesh_construction::construct_square_surface_mesh_fitted_to_canonical_terrain_samples;
use openzt2_zoo_state::terrain::terrain_world_sampling::{sample_terrain, terrain_chunk_at};

use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use openzt2_game_assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;

use openzt2_zoo_state::topology::topology_graph_types::TopologyGrid;

use super::{
    path_presentation_neighborhood::terrain_region,
    topology_construction_planning::fence_definition_is_gate,
};

// Fence mode uses the anchor model and terrain grid decal together.
const ANCHOR_DOCUMENT: &str = "ui/fx/anchor.xml";
const ANCHOR_MODEL: &str = "ui/fx/anchor/anchor.nif";
const GRID_IMAGE: &str = "ui/fx/footprint/diag_card_grid_small.tga";
const GRID_WIDTH_M: f32 = 22.5885;

#[derive(Component, PartialEq)]
pub(super) struct FenceAnchorCell(Option<IVec3>);

pub(super) fn project_fence_anchor_cell(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    archives: Res<AssetArchives>,
    tool: Res<ConstructionTool>,
    active_definitions: Res<WorldDefinitions>,
    definitions: Res<Assets<WorldDefinitionAsset>>,
    grid: Single<&TopologyGrid>,
    ui_capture: Res<openzt2_interface_state::ui::pointer_capture_types::UiPointerCapture>,
    pointer: Query<&ConstructionCursor>,
    roots: Query<Entity, With<WorldRoot>>,
    mut anchors: Query<(Entity, &mut FenceAnchorCell)>,
) {
    let gate = matches!(*tool, ConstructionTool::Fence(definition)
        if active_definitions.get(&definitions)
            .is_some_and(|definitions| fence_definition_is_gate(definitions, definition)));
    let (Ok(pointer), Ok(root), ConstructionTool::Fence(_), false) =
        (pointer.single(), roots.single(), *tool, gate)
    else {
        for (entity, _) in &anchors {
            commands.entity(entity).despawn();
        }
        return;
    };
    let cell = (!ui_capture.over_ui && pointer.over_terrain).then(|| {
        let mut cell = grid.position_cell(pointer.world);
        cell.z = 0;
        cell
    });
    if let Ok((_, mut anchor)) = anchors.single_mut() {
        anchor.set_if_neq(FenceAnchorCell(cell));
        return;
    }
    let entity = commands
        .spawn((
            Name::new("fence anchor cursor"),
            FenceAnchorCell(cell),
            Transform::default(),
            Visibility::Hidden,
            WorldMember { root },
        ))
        .id();
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
}

pub(super) fn fit_fence_anchor_to_terrain(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    grid: Single<Ref<TopologyGrid>>,
    terrain_index: Single<Ref<TerrainIndex>>,
    terrain_assets: Res<Assets<TerrainAsset>>,
    terrain_chunks: Query<(&TerrainChunk, Option<&EditedTerrainSamples>)>,
    changed_chunks: Query<&TerrainChunk>,
    mut changed: MessageReader<TerrainChanged>,
    mut anchors: Query<(
        Entity,
        Ref<FenceAnchorCell>,
        &mut Transform,
        &mut Visibility,
        Option<&Mesh3d>,
    )>,
) {
    let Ok((entity, anchor, mut transform, mut visibility, mesh)) = anchors.single_mut() else {
        changed.clear();
        return;
    };
    let Some(cell) = anchor.0 else {
        changed.clear();
        visibility.set_if_neq(Visibility::Hidden);
        return;
    };
    let mut centre = grid.cell_translation(cell);
    let half_width = Vec2::splat(GRID_WIDTH_M * 0.5);
    let terrain_changed = changed
        .read()
        .filter(|change| change.ground_height_changed)
        .filter_map(|change| terrain_region(change, &changed_chunks))
        .any(|(min, max)| {
            centre.xz().cmpge(min - half_width).all() && centre.xz().cmple(max + half_width).all()
        });
    changed.clear();
    if !anchor.is_changed()
        && !grid.is_changed()
        && !terrain_index.is_changed()
        && !terrain_changed
        && *visibility != Visibility::Hidden
        && mesh.is_some()
    {
        return;
    }
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
        visibility.set_if_neq(Visibility::Hidden);
        return;
    };
    centre.y = height;
    transform.set_if_neq(Transform::from_translation(centre));
    visibility.set_if_neq(Visibility::Inherited);
    let Some(decal) = construct_square_surface_mesh_fitted_to_canonical_terrain_samples(
        GRID_WIDTH_M,
        centre,
        &terrain_index,
        &terrain_assets,
        &terrain_chunks,
    ) else {
        if mesh.is_some() {
            commands
                .entity(entity)
                .remove::<(Mesh3d, MeshMaterial3d<StandardMaterial>)>();
        }
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
