use crate::topology::fence_construction_preview_presentation::*;
use openzt2_game_data::terrain::TerrainGrid;
use openzt2_game_data::terrain::TerrainPresentation;
use openzt2_game_data::terrain::TerrainSample;
use openzt2_game_data::terrain::TerrainSlopeTile;
use openzt2_game_data::terrain::TerrainUnits;
use openzt2_game_data::terrain::TerrainWaterDepth;
use openzt2_game_data::AssetId;

use crate::topology::fence_segment_prefab_refresh::refresh_fence_segment_prefabs_after_topology_index_changes;
use crate::topology::topology_construction_interaction::apply_fence_placement_requests_to_construction_previews;
use crate::topology::topology_construction_interaction::drive_selected_topology_placement_pointer_sequence;
use crate::topology::topology_construction_preview_types::PlaceFence;
use crate::topology::topology_construction_preview_types::PlacePath;
use crate::topology::topology_fence_definition_test_fixture::install_fence_definition_for_topology_transaction_tests;
use bevy::camera::NormalizedRenderTarget;
use bevy::picking::pointer::{Location, PointerAction, PointerButton, PointerId, PointerInput};
use openzt2_game_assets::terrain::terrain_asset_types_and_borrowing_queries::TerrainAsset;
use openzt2_game_assets::terrain::terrain_test_asset::terrain_test_asset;
use openzt2_interface_state::ui::pointer_capture_types::UiPointerCapture;
use openzt2_world_state::world_spawn::world_membership_types::{WorldMember, WorldRoot};
use openzt2_zoo_state::construction::construction_interaction_types::CommitConstruction;
use openzt2_zoo_state::construction::construction_interaction_types::ConstructionCursor;
use openzt2_zoo_state::construction::construction_tool_and_placement_policy_types::ConstructionTool;
use openzt2_zoo_state::terrain::terrain_chunk_types::{
    EditedTerrainSamples, TerrainChunk, TerrainIndex,
};
use openzt2_zoo_state::topology::topology_graph_types::FenceEdge;

fn fence_preview_app() -> App {
    let mut app = App::new();
    app.add_plugins((bevy::app::TaskPoolPlugin::default(), AssetPlugin::default()))
        .init_asset::<super::super::elevated_path_layout::ElevatedPathModeDefinition>()
        .init_asset_loader::<super::super::elevated_path_layout::ElevatedPathModeLoader>()
        .init_resource::<super::super::elevated_path_layout::ElevatedPathModeAsset>()
        .init_resource::<Assets<WorldDefinitionAsset>>()
        .init_resource::<WorldDefinitions>()
        .init_resource::<Assets<TerrainAsset>>()
        .add_message::<PointerInput>()
        .init_resource::<UiPointerCapture>()
        .init_resource::<openzt2_zoo_state::construction::construction_tool_and_placement_policy_types::ConstructionPlacementPolicy>()
        .insert_resource(ConstructionTool::Fence(AssetId::from_key("fence")))
        .add_message::<PlaceFence>()
        .add_message::<PlacePath>()
        .add_message::<CommitConstruction>()
        .add_message::<openzt2_zoo_state::terrain::terrain_edit_types::TerrainChanged>()
        .add_message::<openzt2_zoo_state::topology::topology_edit_types::FenceConnectivityChanged>()
        .add_systems(
            Update,
            (
                drive_selected_topology_placement_pointer_sequence,
                apply_fence_placement_requests_to_construction_previews,
                crate::topology::topology_construction_interaction::evaluate_topology_construction_preview_validity,
                reconcile_fence_construction_preview_presentation_segments,
                refresh_fence_segment_prefabs_after_topology_index_changes,
            )
                .chain(),
        );
    install_fence_definition_for_topology_transaction_tests(&mut app, AssetId::from_key("fence"));
    let asset = app
        .world_mut()
        .resource_mut::<Assets<TerrainAsset>>()
        .add(terrain_test_asset(TerrainGrid {
            width: 9,
            height: 9,
            sector_columns: 1,
            sector_rows: 1,
            units: TerrainUnits {
                cell_size_metres: 1.0,
                base_height_metres: 0.0,
                maximum_height_metres: f32::MAX,
                minimum_water_height_metres: None,
            },
            presentation: TerrainPresentation {
                water_distance_fade: Default::default(),
                water_editing: Default::default(),
                detail_texture: String::new(),
                detail_texture_repetition: 1.0,
                cliff_rise_per_metre: [0.0; 2],
                surface_resolution: 1,
                surface_margin_cells: 0,
                full_alpha_composition: false,
            },
            biomes: Vec::new(),
            samples: vec![
                TerrainSample {
                    height_relative_to_base_metres: 0.0,
                    biome_index: 0,
                    has_ground_cover: false,
                    water_depth: TerrainWaterDepth::Dry,
                    water_surface_flag: false,
                    excludes_elevated_path: false,
                    support_floor_metres: None,
                    height_cap_metres: None,
                    height_is_linked: false,
                    has_child_height_link: false,
                };
                81
            ],
            water_regions: Vec::new(),
            waterfall: None,
            slope_tiles: vec![
                TerrainSlopeTile {
                    uses_alternate_diagonal: false,
                };
                64
            ],
        }));
    let root = app
        .world_mut()
        .spawn((
            WorldRoot {
                scenario: AssetId::default(),
            },
            TopologyIndex::default(),
            TopologyGrid::default(),
        ))
        .id();
    let chunk = app
        .world_mut()
        .spawn((
            WorldMember { root },
            TerrainChunk {
                source_coord: IVec2::ZERO,
                coord: IVec2::ZERO,
                asset,
                origin: Vec2::ZERO,
                spacing_m: 1.0,
                side: 9,
            },
        ))
        .id();
    app.world_mut().entity_mut(root).insert(TerrainIndex {
        chunks: [(IVec2::ZERO, chunk)].into_iter().collect(),
        origin: Vec2::ZERO,
        chunk_span_m: 8.0,
    });
    app
}

fn preview_world_root(app: &mut App) -> Entity {
    app.world_mut()
        .query_filtered::<Entity, With<WorldRoot>>()
        .single(app.world())
        .unwrap()
}

fn insert_fence(app: &mut App, definition: AssetId, first: IVec3, second: IVec3) -> Entity {
    let root = preview_world_root(app);
    let [a, b] = [first, second].map(|cell| {
        if let Some(entity) = app
            .world()
            .get::<TopologyIndex>(root)
            .unwrap()
            .nodes
            .get(&cell)
        {
            return *entity;
        }
        app.world_mut()
            .spawn((TopologyNode { cell }, WorldMember { root }))
            .id()
    });
    app.world_mut()
        .spawn((
            FenceEdge::between(definition, a, b),
            WorldMember { root },
            Transform::IDENTITY,
        ))
        .id()
}

fn spawn_preview(app: &mut App, from: IVec3, to: IVec3) -> Entity {
    let root = preview_world_root(app);
    let grid = *app.world().get::<TopologyGrid>(root).unwrap();
    let cursor = app
        .world_mut()
        .spawn((
            WorldMember { root },
            ConstructionCursor {
                world: grid.cell_translation(from),
                normal: Vec3::Y,
                over_terrain: true,
            },
        ))
        .id();
    app.world_mut().write_message(PointerInput::new(
        PointerId::Mouse,
        Location {
            target: NormalizedRenderTarget::None {
                width: 1,
                height: 1,
            },
            position: Vec2::ZERO,
        },
        PointerAction::Press(PointerButton::Primary),
    ));
    app.update();
    app.world_mut()
        .get_mut::<ConstructionCursor>(cursor)
        .unwrap()
        .world = grid.cell_translation(to);
    app.world_mut()
        .query_filtered::<Entity, With<FencePreview>>()
        .single(app.world())
        .expect("native click created anchor")
}

fn pending_segment(app: &mut App) -> (Entity, FenceSegmentPrefab, Transform) {
    let mut query = app.world_mut().query_filtered::<
        (Entity, &FenceSegmentPrefab, &Transform),
        With<FenceConstructionPreviewPresentationSegment>,
    >();
    let (entity, prefab, transform) = query.single(app.world()).expect("one pending segment");
    (entity, *prefab, *transform)
}

#[test]
fn pending_junction_segment_matches_committed_curve_and_transform() {
    let corner = IVec3::new(4, 4, 0);
    for (outgoing, expected) in [
        (IVec3::Y, "test/cardinal_curve_90"),
        (-IVec3::Y, "test/cardinal_curve_90"),
        (IVec3::new(1, 1, 0), "test/diagonal_curve_135"),
        (IVec3::new(1, -1, 0), "test/diagonal_curve_135"),
        (IVec3::X, "test/cardinal_straight"),
    ] {
        let mut app = fence_preview_app();
        let definition = AssetId::from_key("fence");
        insert_fence(&mut app, definition, corner - IVec3::X, corner);
        let preview = spawn_preview(&mut app, corner, corner + outgoing);
        app.update();
        let (_, pending_prefab, pending_transform) = pending_segment(&mut app);
        assert_eq!(pending_prefab.asset, AssetId::from_key(expected));
        // Materialize the accepted endpoints as the transaction does, then use
        // the production committed-graph refresh to compare presentation.
        app.world_mut().despawn(preview);
        let committed = insert_fence(&mut app, definition, corner, corner + outgoing);
        app.update();
        assert_eq!(
            app.world().get::<FenceSegmentPrefab>(committed),
            Some(&pending_prefab),
        );
        assert_eq!(
            app.world().get::<Transform>(committed),
            Some(&pending_transform),
        );
        assert_eq!(
            app.world_mut()
                .query::<&FenceConstructionPreviewPresentationSegment>()
                .iter(app.world())
                .count(),
            0,
        );
    }
}

#[test]
fn a_fence_built_over_a_broken_segment_replaces_it_and_another_fence_occupies_the_run() {
    use crate::topology::topology_construction_planning::{
        calculate_fence_construction_plan, GateHostEdges,
    };
    use crate::topology::topology_construction_preview_types::FencePreview;
    use bevy::ecs::system::RunSystemOnce;
    use openzt2_zoo_state::construction::construction_tool_and_placement_policy_types::FencePlacementShape;
    let mut app = fence_preview_app();
    let broken = AssetId::from_key("fence_broken");
    crate::topology::topology_fence_definition_test_fixture::install_fence_definition_with_broken_variant(
        &mut app,
        AssetId::from_key("fence"),
        Some(broken),
    );
    let gap = insert_fence(&mut app, broken, IVec3::new(2, 2, 0), IVec3::new(3, 2, 0));
    let plan = |app: &mut App, to: i32| {
        app.world_mut()
            .run_system_once(
                move |root: Single<(&TopologyIndex, &TopologyGrid, &TerrainIndex)>,
                      definitions: Res<Assets<WorldDefinitionAsset>>,
                      active: Res<WorldDefinitions>,
                      terrain_assets: Res<Assets<TerrainAsset>>,
                      chunks: Query<(&TerrainChunk, Option<&EditedTerrainSamples>)>,
                      hosts: GateHostEdges| {
                    let (index, grid, terrain) = root.into_inner();
                    calculate_fence_construction_plan(
                        &FencePreview {
                            definition: AssetId::from_key("fence"),
                            from: IVec3::new(1, 2, 0),
                            to: IVec3::new(to, 2, 0),
                            shape: FencePlacementShape::Freeform,
                            heading_eighth_turns: 0,
                        },
                        &index,
                        *grid,
                        active.get(&definitions).unwrap(),
                        &terrain,
                        &terrain_assets,
                        &chunks,
                        &hosts,
                        None,
                    )
                    .map(|(.., replaced)| replaced)
                },
            )
            .unwrap()
    };
    // A run across the broken segment replaces it.
    assert_eq!(plan(&mut app, 4), Ok(vec![gap]));
    // A run over a sound segment is refused.
    insert_fence(
        &mut app,
        AssetId::from_key("fence"),
        IVec3::new(4, 2, 0),
        IVec3::new(5, 2, 0),
    );
    assert_eq!(
        plan(&mut app, 6),
        Err(openzt2_zoo_state::construction::construction_interaction_types::PlacementFailure::Occupied)
    );
}
