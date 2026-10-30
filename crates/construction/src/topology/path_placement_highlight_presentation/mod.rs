use super::{
    topology_construction_planning::{
        calculate_ground_path_construction_cells, calculate_path_construction_plan,
    },
    topology_construction_preview_types::PathPreview,
};
use bevy::{
    camera::visibility::RenderLayers,
    platform::collections::{HashMap, HashSet},
    prelude::*,
};
use openzt2_game_assets::{
    terrain::terrain_asset_types_and_borrowing_queries::TerrainAsset,
    world_definitions::{
        path_placement_highlight_asset::PathPlacementHighlightAsset,
        world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions,
        world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset,
    },
};
use openzt2_game_data::AssetId;
use openzt2_interface_state::ui::pointer_capture_types::UiPointerCapture;
use openzt2_world_state::world_spawn::world_membership_types::{WorldMember, WorldRoot};
use openzt2_zoo_state::{
    construction::{
        construction_interaction_types::{
            ConstructionCursor, ConstructionPreview, PlacementValidity,
        },
        construction_tool_and_placement_policy_types::ConstructionTool,
    },
    terrain::{
        terrain_chunk_presentation_types::TERRAIN_RENDER_LAYER,
        terrain_chunk_types::{EditedTerrainSamples, TerrainChunk, TerrainIndex},
        terrain_edit_types::TerrainChanged,
        terrain_fitted_surface_mesh_construction::construct_square_surface_mesh_fitted_to_canonical_terrain_samples_with_rotated_texture_coordinates,
    },
    topology::{
        ground_path_layout_calculation::calculate_snapped_ground_path_layout,
        topology_graph_types::{TopologyGrid, TopologyIndex},
    },
};

#[derive(Resource)]
pub(super) struct PathPlacementHighlightAssets {
    rules: Handle<PathPlacementHighlightAsset>,
    materials: HashMap<(bevy::asset::AssetId<Image>, bool), Handle<StandardMaterial>>,
}

impl FromWorld for PathPlacementHighlightAssets {
    fn from_world(world: &mut World) -> Self {
        Self {
            rules: world
                .resource::<AssetServer>()
                .load("ui/modes/pathcurves.xml"),
            materials: HashMap::default(),
        }
    }
}

#[derive(Component)]
pub(super) struct PathPlacementHighlight {
    definition: AssetId,
    cell: IVec3,
}

#[allow(
    clippy::too_many_arguments,
    clippy::too_many_lines,
    clippy::needless_pass_by_value,
    reason = "Bevy supplies owned system parameters; one reconciliation pass updates retained decals and creates newly exposed cells"
)]
pub(super) fn present_ground_path_placement_highlights(
    mut commands: Commands,
    tool: Res<ConstructionTool>,
    capture: Res<UiPointerCapture>,
    (grid, index): (Res<TopologyGrid>, Res<TopologyIndex>),
    (active_definitions, definitions): (Res<WorldDefinitions>, Res<Assets<WorldDefinitionAsset>>),
    (rules, mut presentation): (
        Res<Assets<PathPlacementHighlightAsset>>,
        ResMut<PathPlacementHighlightAssets>,
    ),
    terrain: Res<TerrainIndex>,
    terrain_assets: Res<Assets<TerrainAsset>>,
    terrain_chunks: Query<(&TerrainChunk, Option<&EditedTerrainSamples>)>,
    mut terrain_changes: MessageReader<TerrainChanged>,
    cursors: Query<&ConstructionCursor>,
    roots: Query<Entity, With<WorldRoot>>,
    previews: Query<(&PathPreview, &ConstructionPreview)>,
    mut highlights: Query<(
        Entity,
        &PathPlacementHighlight,
        &Mesh3d,
        &mut MeshMaterial3d<StandardMaterial>,
        &mut Visibility,
    )>,
    (mut meshes, mut materials): (ResMut<Assets<Mesh>>, ResMut<Assets<StandardMaterial>>),
) {
    let refit_surfaces = terrain_changes.read().count() != 0;
    let Some(definitions) = active_definitions.get(&definitions) else {
        return;
    };
    let selected = match *tool {
        ConstructionTool::Path(id) => definitions.find_path(id).filter(|path| !path.elevated),
        _ => None,
    };
    let Some(definition) = selected else {
        for (entity, ..) in &highlights {
            commands.entity(entity).despawn();
        }
        return;
    };
    let (Ok(cursor), Ok(root), Some(rules)) = (
        cursors.single(),
        roots.single(),
        rules.get(&presentation.rules),
    ) else {
        return;
    };
    if capture.over_ui || !cursor.over_terrain {
        for (_, _, _, _, mut visibility) in &mut highlights {
            visibility.set_if_neq(Visibility::Hidden);
        }
        return;
    }
    let cell = grid.position_cell(cursor.world).with_z(0);
    let hovered = PathPreview {
        definition: definition.id,
        from: cell,
        to: cell,
    };
    let (preview, validity) = previews
        .single()
        .ok()
        .filter(|(path, _)| path.definition == definition.id)
        .map_or((&hovered, None), |(path, construction)| {
            (path, Some(construction.validity))
        });
    let Ok(cells) = calculate_ground_path_construction_cells(
        preview.from,
        preview.to,
        definition.width_cm,
        definitions.topology_cell_size_cm(),
        &index,
    ) else {
        return;
    };
    let valid = !matches!(validity, Some(PlacementValidity::Invalid(_)))
        && calculate_path_construction_plan(
            preview,
            &index,
            *grid,
            definitions,
            &terrain,
            &terrain_assets,
            &terrain_chunks,
        )
        .is_ok();
    let desired: HashSet<_> = cells.into_iter().collect();
    let mut retained = HashSet::default();
    for (entity, highlight, mesh, mut material, mut visibility) in &mut highlights {
        if highlight.definition != definition.id || !desired.contains(&highlight.cell) {
            commands.entity(entity).despawn();
            continue;
        }
        let Some((texture, rotation)) = highlight_texture_and_rotation(
            rules,
            highlight.cell,
            definition.width_cm,
            definitions.topology_cell_size_cm(),
            &desired,
            &index,
        ) else {
            continue;
        };
        let selected_material =
            highlight_material(texture, valid, &mut presentation.materials, &mut materials);
        if material.0 != selected_material {
            material.0 = selected_material;
        }
        visibility.set_if_neq(Visibility::Inherited);
        if refit_surfaces {
            let centre = grid.cell_translation(highlight.cell);
            if let Some(replacement) = construct_square_surface_mesh_fitted_to_canonical_terrain_samples_with_rotated_texture_coordinates(f32::from(definition.width_cm) * 0.04, centre, rotation, &terrain, &terrain_assets, &terrain_chunks) {
                if let Some(mut mesh) = meshes.get_mut(&mesh.0) { *mesh = replacement; }
            }
        }
        retained.insert(highlight.cell);
    }
    for cell in desired.difference(&retained).copied() {
        let Some((texture, rotation)) = highlight_texture_and_rotation(
            rules,
            cell,
            definition.width_cm,
            definitions.topology_cell_size_cm(),
            &desired,
            &index,
        ) else {
            continue;
        };
        let centre = grid.cell_translation(cell);
        let Some(mesh) = construct_square_surface_mesh_fitted_to_canonical_terrain_samples_with_rotated_texture_coordinates(f32::from(definition.width_cm) * 0.04, centre, rotation, &terrain, &terrain_assets, &terrain_chunks) else { continue; };
        let material =
            highlight_material(texture, valid, &mut presentation.materials, &mut materials);
        commands.spawn((
            PathPlacementHighlight {
                definition: definition.id,
                cell,
            },
            Mesh3d(meshes.add(mesh)),
            MeshMaterial3d(material),
            Transform::from_translation(centre),
            Visibility::Inherited,
            RenderLayers::layer(TERRAIN_RENDER_LAYER),
            WorldMember { root },
        ));
    }
}

fn highlight_texture_and_rotation(
    rules: &PathPlacementHighlightAsset,
    cell: IVec3,
    width_cm: u16,
    spacing_cm: u16,
    planned: &HashSet<IVec3>,
    index: &TopologyIndex,
) -> Option<(Handle<Image>, f32)> {
    let (centre, width, _) = calculate_snapped_ground_path_layout(cell, width_cm, spacing_cm)?;
    let side = cell - centre;
    let direction = match side.xy() {
        side if side == -IVec2::Y * (width / 2) => 0,
        side if side == IVec2::X * (width / 2) => 2,
        side if side == IVec2::Y * (width / 2) => 4,
        side if side == -IVec2::X * (width / 2) => 6,
        _ => return None,
    };
    let texture = rules.texture_for_direction_cell(centre, direction, width / 2, |cell| {
        planned.contains(&cell)
            || index.paths.contains_key(&cell)
            || calculate_snapped_ground_path_layout(cell, width_cm, spacing_cm)
                .is_some_and(|(centre, _, _)| index.paths.contains_key(&centre))
    });
    let turns = ((direction + 2) % 8) / 2;
    Some((texture, -f32::from(turns) * std::f32::consts::FRAC_PI_2))
}

fn highlight_material(
    texture: Handle<Image>,
    valid: bool,
    cache: &mut HashMap<(bevy::asset::AssetId<Image>, bool), Handle<StandardMaterial>>,
    materials: &mut Assets<StandardMaterial>,
) -> Handle<StandardMaterial> {
    cache
        .entry((texture.id(), valid))
        .or_insert_with(|| {
            materials.add(StandardMaterial {
                base_color: if valid {
                    Color::srgb(0.0, 1.0, 0.0)
                } else {
                    Color::srgb(1.0, 0.0, 0.0)
                },
                base_color_texture: Some(texture),
                alpha_mode: AlphaMode::Blend,
                unlit: true,
                depth_bias: 1.0,
                ..default()
            })
        })
        .clone()
}
