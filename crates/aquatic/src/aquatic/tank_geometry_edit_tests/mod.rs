use std::time::Duration;

use bevy::{ecs::schedule::ScheduleLabel, prelude::*};
use openzt2_game_data::{
    world_definitions::{
        aquatic_habitats::{TankDefinition, TankDepthPolicy, TankEditPolicy, TankSurfacePolicy},
        document::WorldDefinitionDocument,
        fences_and_gates::FencePlacementModePolicy,
    },
    AssetId,
};

use openzt2_construction::construction::ConstructionPlugin;
use openzt2_game_assets::terrain::terrain_asset_types_and_borrowing_queries::TerrainAsset;
use openzt2_game_assets::terrain::terrain_test_asset::terrain_test_asset;
use openzt2_game_assets::{
    application_lifecycle::GamePhase,
    world_definitions::{
        world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions,
        world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset,
    },
};
use openzt2_game_data::terrain::{
    TerrainGrid, TerrainPresentation, TerrainSample, TerrainSlopeTile, TerrainUnits,
    TerrainWaterDepth,
};
use openzt2_interface_state::input::input_types::PrimaryPointerInputState;
use openzt2_interface_state::ui::input_frame_capture_types::AuthoredModalInputCapture;
use openzt2_interface_state::ui::pointer_capture_types::UiPointerCapture;
use openzt2_world_state::world_spawn::world_membership_types::WorldMember;
use openzt2_world_state::world_spawn::world_membership_types::WorldRoot;
use openzt2_zoo_state::construction::construction_edit_history_types::ApplyConstructionTransaction;
use openzt2_zoo_state::construction::construction_edit_history_types::ConstructionEditDirection;
use openzt2_zoo_state::construction::construction_edit_history_types::ConstructionEditHistory;
use openzt2_zoo_state::construction::construction_interaction_types::ConstructionCursor;
use openzt2_zoo_state::construction::construction_tool_and_placement_policy_types::ConstructionPlacementPolicy;
use openzt2_zoo_state::construction::construction_tool_and_placement_policy_types::ConstructionTool;
use openzt2_zoo_state::construction::construction_tool_and_placement_policy_types::TankEditKind;
use openzt2_zoo_state::construction::FixedConstructionPreparedDomainEditMaterializationSet;
use openzt2_zoo_state::construction::FixedConstructionSet;
use openzt2_zoo_state::construction::FixedConstructionTransactionMaterializationSet;
use openzt2_zoo_state::economy::account_transaction_types::TransactionCompleted;
use openzt2_zoo_state::economy::account_transaction_types::TransactionRejected;
use openzt2_zoo_state::economy::account_transaction_types::TransactionRequest;
use openzt2_zoo_state::habitat::habitat_types::HabitatIndex;
use openzt2_zoo_state::habitat::habitat_types::HabitatRegion;
use openzt2_zoo_state::habitat::habitat_types::HabitatSummary;
use openzt2_zoo_state::placement::placement_transaction_types::ApplyPreparedObjectPlacementEditRequest;
use openzt2_zoo_state::placement::placement_transaction_types::ObjectPlacementEditApplicationAcknowledged;
use openzt2_zoo_state::placement::placement_transaction_types::ObjectPlacementEditPreparationRejected;
use openzt2_zoo_state::placement::placement_transaction_types::ObjectPlacementEditPrepared;
use openzt2_zoo_state::shows::show_platform_upgrade_types::CommitShowPlatformUpgradeEdit;
use openzt2_zoo_state::shows::show_platform_upgrade_types::ShowPlatformUpgradeEditAcknowledged;
use openzt2_zoo_state::terrain::terrain_chunk_types::{TerrainChunk, TerrainIndex};
use openzt2_zoo_state::terrain::terrain_edit_types::CommitTerrainEdit;
use openzt2_zoo_state::terrain::terrain_edit_types::TerrainEditAcknowledged;
use openzt2_zoo_state::terrain::terrain_edit_types::TerrainEditPreparationRejected;
use openzt2_zoo_state::terrain::terrain_edit_types::TerrainEditPrepared;
use openzt2_zoo_state::topology::topology_edit_types::CommitTopologyEdit;
use openzt2_zoo_state::topology::topology_edit_types::TopologyEditAcknowledged;
use openzt2_zoo_state::topology::topology_edit_types::TopologyEditPreparationRejected;
use openzt2_zoo_state::topology::topology_edit_types::TopologyEditPrepared;
use openzt2_zoo_state::topology::topology_graph_types::{FenceEdge, TopologyGrid, TopologyNode};
use openzt2_zoo_state::transport_tours::transport_track_construction_types::ApplyPreparedTransportTrackConstructionEditRequest;
use openzt2_zoo_state::transport_tours::transport_track_construction_types::TransportTrackConstructionEditApplicationAcknowledged;
use openzt2_zoo_state::transport_tours::transport_track_construction_types::TransportTrackConstructionEditPreparationRejected;
use openzt2_zoo_state::transport_tours::transport_track_construction_types::TransportTrackConstructionEditPrepared;

use super::{
    aquatic_simulation_types::{ShowTankEligible, Tank, TankGeometry},
    show_tank_eligibility_projection::project_changed_tank_depth_into_show_eligibility,
    tank_geometry_edit_execution::{
        apply_tank_geometry_construction_edits, prepare_tank_geometry_construction_edits,
    },
    tank_geometry_edit_interaction::update_tank_geometry_gesture,
    tank_geometry_edit_types::{
        CommitTankGeometryEdit, TankGeometryEditAcknowledged, TankGeometryEditPreparationRejected,
        TankGeometryEditPrepared,
    },
};

#[derive(ScheduleLabel, Debug, Clone, PartialEq, Eq, Hash)]
struct TankPointerStep;
#[derive(ScheduleLabel, Debug, Clone, PartialEq, Eq, Hash)]
struct TankProjectionStep;

fn tank_geometry_construction_fixture(kind: TankEditKind) -> (App, Entity, Entity) {
    let mut app = App::new();
    app.insert_resource(State::new(GamePhase::InGame))
        .init_resource::<Time>()
        .init_resource::<Assets<WorldDefinitionAsset>>()
        .init_resource::<WorldDefinitions>()
        .init_resource::<PrimaryPointerInputState>()
        .init_resource::<UiPointerCapture>()
        .init_resource::<AuthoredModalInputCapture>()
        .init_resource::<TopologyGrid>()
        .init_resource::<HabitatIndex>()
        .init_resource::<Assets<TerrainAsset>>()
        .add_message::<CommitTankGeometryEdit>()
        .add_message::<TankGeometryEditPrepared>()
        .add_message::<TankGeometryEditPreparationRejected>()
        .add_message::<TankGeometryEditAcknowledged>()
        .add_message::<TransactionRequest>()
        .add_message::<TransactionCompleted>()
        .add_message::<TransactionRejected>()
        .add_message::<CommitTerrainEdit>()
        .add_message::<TerrainEditPrepared>()
        .add_message::<TerrainEditPreparationRejected>()
        .add_message::<TerrainEditAcknowledged>()
        .add_message::<CommitTopologyEdit>()
        .add_message::<TopologyEditPrepared>()
        .add_message::<TopologyEditPreparationRejected>()
        .add_message::<TopologyEditAcknowledged>()
        .add_message::<ApplyPreparedObjectPlacementEditRequest>()
        .add_message::<ObjectPlacementEditPrepared>()
        .add_message::<ObjectPlacementEditPreparationRejected>()
        .add_message::<ObjectPlacementEditApplicationAcknowledged>()
        .add_message::<CommitShowPlatformUpgradeEdit>()
        .add_message::<ShowPlatformUpgradeEditAcknowledged>()
        .add_message::<ApplyPreparedTransportTrackConstructionEditRequest>()
        .add_message::<TransportTrackConstructionEditPrepared>()
        .add_message::<TransportTrackConstructionEditPreparationRejected>()
        .add_message::<TransportTrackConstructionEditApplicationAcknowledged>()
        .add_plugins(ConstructionPlugin)
        .insert_resource(ConstructionTool::TankEdit)
        .insert_resource(ConstructionPlacementPolicy {
            tank_edit: kind,
            ..default()
        })
        .add_systems(TankPointerStep, update_tank_geometry_gesture)
        .add_systems(
            TankProjectionStep,
            project_changed_tank_depth_into_show_eligibility,
        )
        .add_systems(
            FixedUpdate,
            prepare_tank_geometry_construction_edits
                .in_set(FixedConstructionSet::Prepare)
                .after(FixedConstructionTransactionMaterializationSet)
                .before(FixedConstructionPreparedDomainEditMaterializationSet),
        )
        .add_systems(
            FixedUpdate,
            apply_tank_geometry_construction_edits
                .in_set(FixedConstructionSet::Apply)
                .after(openzt2_zoo_state::construction::ConstructionFailureRollbackDispatchSet),
        );
    let definition = AssetId::from_key("test/tank");
    let document = WorldDefinitionDocument {
        tanks: vec![TankDefinition {
            id: definition,
            wall: AssetId::default(),
            floor_material: AssetId::default(),
            water_material: AssetId::default(),
            min_depth_cm: 100,
            max_depth_cm: 500,
            capacity_litres_per_cell: 1000,
            filtration_per_day: 0,
            opaque_wall_material: None,
        }],
        tank_surface_policy: Some(TankSurfacePolicy {
            water_height_offset_cm: 0,
            floor_height_offset_cm: 0,
        }),
        fence_placement_mode_policy: Some(FencePlacementModePolicy {
            minimum_rectangle_segments: 1,
            minimum_octagon_segments: 3,
            default_tank_wall_height_cm: 400,
            minimum_tank_wall_height_cm: 0,
            hill_threshold_cm: 600,
        }),
        tank_depth_policy: Some(TankDepthPolicy {
            minimum_water_depth_cm: 100,
            minimum_show_tank_depth_cm: 300,
        }),
        tank_edit_policy: Some(TankEditPolicy {
            floor_mm_per_second: 1000,
            wall_mm_per_second: 1000,
            water_mm_per_second: 1000,
            fast_multiplier_q16: 65536,
            slow_multiplier_q16: 65536,
            fill_speed_fraction_permille: [0, 1000],
            drain_speed_fraction_permille: [0, 1000],
        }),
        ..default()
    };
    let handle = app
        .world_mut()
        .resource_mut::<Assets<WorldDefinitionAsset>>()
        .add(WorldDefinitionAsset::from_test_document(document.clone()));
    app.world_mut()
        .resource_mut::<WorldDefinitions>()
        .index_test_document(handle, &document);
    let root = app
        .world_mut()
        .spawn(WorldRoot {
            scenario: AssetId::default(),
        })
        .id();
    let geometry = TankGeometry {
        floor_height: 0.0,
        wall_height: 4.0,
        water_height: 2.0,
        water_height_limit: 1_000_000.0,
        wall_base_height: 0.0,
        area: 4.0,
        volume: 8.0,
    };
    install_flat_terrain(&mut app);
    let corners = [
        IVec3::ZERO,
        IVec3::new(2, 0, 0),
        IVec3::new(2, 2, 0),
        IVec3::new(0, 2, 0),
    ]
    .map(|cell| app.world_mut().spawn(TopologyNode { cell }).id());
    let walls = (0..4)
        .map(|index| {
            app.world_mut()
                .spawn((
                    FenceEdge {
                        definition,
                        a: corners[index],
                        b: corners[(index + 1) % 4],
                    },
                    openzt2_world_state::world_spawn::persistent_id_types::PersistentId(
                        100 + index as u64,
                    ),
                    super::aquatic_simulation_types::TankWallTopHeight(geometry.wall_height),
                ))
                .id()
        })
        .collect::<Box<[_]>>();
    let tank = app
        .world_mut()
        .spawn((
            Tank { definition },
            geometry,
            WorldMember { root },
            HabitatRegion {
                topology_cells: vec![IVec2::ZERO, IVec2::X, IVec2::Y, IVec2::ONE]
                    .into_boxed_slice(),
                area_square_metres: 4.0,
                boundary_fence_entities: walls,
            },
            HabitatSummary {
                land_area_square_metres: 0.0,
                water_area_square_metres: 4.0,
                biome_areas_square_metres: Box::default(),
                boundary_is_breached: false,
            },
        ))
        .id();
    let other = app.world_mut().spawn((Tank { definition }, geometry)).id();
    let mut habitat_index = app.world_mut().resource_mut::<HabitatIndex>();
    for cell in [IVec2::ZERO, IVec2::X, IVec2::Y, IVec2::ONE] {
        habitat_index.record_habitat_for_topology_cell(cell, tank);
    }
    app.world_mut().spawn(ConstructionCursor {
        world: Vec3::new(0.5, 0.0, 0.5),
        normal: Vec3::Y,
        over_terrain: true,
    });
    (app, tank, other)
}

fn install_flat_terrain(app: &mut App) {
    let asset = app
        .world_mut()
        .resource_mut::<Assets<TerrainAsset>>()
        .add(terrain_test_asset(TerrainGrid {
            width: 5,
            height: 5,
            sector_columns: 1,
            sector_rows: 1,
            units: TerrainUnits {
                cell_size_metres: 1.0,
                base_height_metres: 0.0,
                maximum_height_metres: 20.0,
                minimum_water_height_metres: Some(-10.0),
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
                25
            ],
            water_regions: Vec::new(),
            waterfall: None,
            slope_tiles: vec![
                TerrainSlopeTile {
                    uses_alternate_diagonal: false,
                };
                16
            ],
        }));
    let chunk = app
        .world_mut()
        .spawn(TerrainChunk {
            source_coord: IVec2::ZERO,
            coord: IVec2::ZERO,
            asset,
            origin: Vec2::ZERO,
            spacing_m: 1.0,
            side: 5,
        })
        .id();
    app.insert_resource(TerrainIndex {
        chunks: [(IVec2::ZERO, chunk)].into_iter().collect(),
        origin: Vec2::ZERO,
        chunk_span_m: 4.0,
    });
}

fn pointer_step(
    app: &mut App,
    pressed: bool,
    just_pressed: bool,
    just_released: bool,
    seconds: f32,
) {
    app.world_mut().insert_resource(PrimaryPointerInputState {
        pressed,
        just_pressed,
        just_released,
        ..default()
    });
    app.world_mut()
        .resource_mut::<Time>()
        .advance_by(Duration::from_secs_f32(seconds));
    app.world_mut().run_schedule(TankPointerStep);
}

#[test]
fn floor_release_commits_once_and_history_reverses_canonical_geometry_and_show_eligibility() {
    let (mut app, tank, other) = tank_geometry_construction_fixture(TankEditKind::LowerFloor);
    let before = *app.world().get::<TankGeometry>(tank).unwrap();
    pointer_step(&mut app, true, true, false, 0.0);
    pointer_step(&mut app, true, false, false, 1.0);
    assert_eq!(*app.world().get::<TankGeometry>(tank).unwrap(), before);
    pointer_step(&mut app, false, false, true, 0.0);
    app.world_mut().run_schedule(FixedUpdate);
    app.world_mut().run_schedule(TankProjectionStep);
    let after = *app.world().get::<TankGeometry>(tank).unwrap();
    assert_eq!(
        after,
        TankGeometry {
            floor_height: -1.0,
            volume: 12.0,
            ..before
        }
    );
    assert!(app.world().get::<ShowTankEligible>(tank).is_some());
    assert_eq!(*app.world().get::<TankGeometry>(other).unwrap(), before);
    let history = app.world().resource::<ConstructionEditHistory>();
    assert_eq!(history.entries.len(), 1);
    assert_eq!(history.cursor, 1);
    let transaction = history.entries[0];
    app.world_mut().run_schedule(FixedUpdate);
    assert_eq!(
        app.world()
            .resource::<ConstructionEditHistory>()
            .entries
            .len(),
        1
    );
    for (direction, expected, cursor) in [
        (ConstructionEditDirection::Undo, before, 0),
        (ConstructionEditDirection::Redo, after, 1),
    ] {
        app.world_mut().write_message(ApplyConstructionTransaction {
            transaction,
            direction,
        });
        app.world_mut().run_schedule(FixedUpdate);
        app.world_mut().run_schedule(FixedUpdate);
        app.world_mut().run_schedule(TankProjectionStep);
        assert_eq!(*app.world().get::<TankGeometry>(tank).unwrap(), expected);
        assert_eq!(
            app.world().resource::<ConstructionEditHistory>().cursor,
            cursor
        );
        assert_eq!(
            app.world().get::<ShowTankEligible>(tank).is_some(),
            cursor == 1
        );
    }
    assert!(app
        .world()
        .resource::<Messages<TransactionRequest>>()
        .is_empty());
}
