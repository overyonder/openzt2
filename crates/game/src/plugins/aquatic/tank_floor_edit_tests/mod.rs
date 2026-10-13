use std::time::Duration;

use bevy::{ecs::schedule::ScheduleLabel, prelude::*};
use openzt2_game_data::{
    world_definitions::{
        aquatic_habitats::{TankDefinition, TankDepthPolicy, TankEditPolicy},
        document::WorldDefinitionDocument,
    },
    AssetId,
};

use crate::plugins::{
    construction::{
        construction_edit_history_types::{
            ApplyConstructionTransaction, ConstructionEditDirection, ConstructionEditHistory,
        },
        construction_interaction_types::ConstructionCursor,
        construction_tool_and_placement_policy_types::{
            ConstructionPlacementPolicy, ConstructionTool, TankEditKind,
        },
        ConstructionPlugin, FixedConstructionPreparedDomainEditMaterializationSet,
        FixedConstructionSet, FixedConstructionTransactionMaterializationSet,
    },
    economy::account_transaction_types::{
        TransactionCompleted, TransactionRejected, TransactionRequest,
    },
    habitat::habitat_types::{HabitatRegion, HabitatSummary},
    input::input_types::PrimaryPointerInputState,
    placement::placement_transaction_types::{
        ApplyPreparedObjectPlacementEditRequest, ObjectPlacementEditApplicationAcknowledged,
        ObjectPlacementEditPreparationRejected, ObjectPlacementEditPrepared,
    },
    shows::show_platform_upgrade_types::{
        CommitShowPlatformUpgradeEdit, ShowPlatformUpgradeEditAcknowledged,
    },
    terrain::terrain_edit_types::{
        CommitTerrainEdit, TerrainEditAcknowledged, TerrainEditPreparationRejected,
        TerrainEditPrepared,
    },
    topology::{
        topology_edit_types::{
            CommitTopologyEdit, TopologyEditAcknowledged, TopologyEditPreparationRejected,
            TopologyEditPrepared,
        },
        topology_graph_types::TopologyGrid,
    },
    transport_tours::transport_track_construction_types::{
        ApplyPreparedTransportTrackConstructionEditRequest,
        TransportTrackConstructionEditApplicationAcknowledged,
        TransportTrackConstructionEditPreparationRejected, TransportTrackConstructionEditPrepared,
    },
    ui::{active_authored_ui_context::AuthoredModalInputCapture, picking::UiPointerCapture},
    world_spawn::world_membership_types::{WorldMember, WorldRoot},
};
use openzt2_game_assets::{
    application_lifecycle::GamePhase,
    world_definitions::{
        world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions,
        world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset,
    },
};

use super::{
    aquatic_simulation_types::{HydratedTankSurface, ShowTankEligible, Tank, TankGeometry},
    show_tank_eligibility_projection::project_changed_tank_depth_into_show_eligibility,
    tank_floor_edit_execution::{
        apply_tank_floor_construction_edits, prepare_tank_floor_construction_edits,
    },
    tank_floor_edit_interaction::update_tank_floor_gesture,
    tank_floor_edit_types::{
        CommitTankFloorEdit, TankFloorEditAcknowledged, TankFloorEditPreparationRejected,
        TankFloorEditPrepared,
    },
};

#[derive(ScheduleLabel, Debug, Clone, PartialEq, Eq, Hash)]
struct TankPointerStep;
#[derive(ScheduleLabel, Debug, Clone, PartialEq, Eq, Hash)]
struct TankProjectionStep;

fn tank_floor_construction_fixture(kind: TankEditKind) -> (App, Entity, Entity) {
    let mut app = App::new();
    app.insert_resource(State::new(GamePhase::InGame))
        .init_resource::<Time>()
        .init_resource::<Assets<WorldDefinitionAsset>>()
        .init_resource::<WorldDefinitions>()
        .init_resource::<PrimaryPointerInputState>()
        .init_resource::<UiPointerCapture>()
        .init_resource::<AuthoredModalInputCapture>()
        .init_resource::<TopologyGrid>()
        .add_message::<CommitTankFloorEdit>()
        .add_message::<TankFloorEditPrepared>()
        .add_message::<TankFloorEditPreparationRejected>()
        .add_message::<TankFloorEditAcknowledged>()
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
        .insert_resource(ConstructionPlacementPolicy { tank_edit: kind, ..default() })
        .add_systems(TankPointerStep, update_tank_floor_gesture)
        .add_systems(TankProjectionStep, project_changed_tank_depth_into_show_eligibility)
        .add_systems(FixedUpdate, prepare_tank_floor_construction_edits
            .in_set(FixedConstructionSet::Prepare)
            .after(FixedConstructionTransactionMaterializationSet)
            .before(FixedConstructionPreparedDomainEditMaterializationSet))
        .add_systems(FixedUpdate, apply_tank_floor_construction_edits
            .in_set(FixedConstructionSet::Apply)
            .after(crate::plugins::construction::construction_failure_rollback_dispatch::dispatch_construction_failure_rollback_to_changed_domains_once));
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
        }],
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
        area: 4.0,
        volume: 8.0,
    };
    let tank = app
        .world_mut()
        .spawn((
            Tank { definition },
            HydratedTankSurface::restored(geometry),
            WorldMember { root },
            HabitatRegion {
                topology_cells: vec![IVec2::ZERO, IVec2::X, IVec2::Y, IVec2::ONE]
                    .into_boxed_slice(),
                area_square_metres: 4.0,
                boundary_fence_entities: Box::default(),
            },
            HabitatSummary {
                land_area_square_metres: 0.0,
                water_area_square_metres: 4.0,
                biome_areas_square_metres: Box::default(),
                boundary_is_breached: false,
            },
        ))
        .id();
    let other = app
        .world_mut()
        .spawn((Tank { definition }, HydratedTankSurface::restored(geometry)))
        .id();
    app.world_mut().spawn(ConstructionCursor {
        world: Vec3::new(0.5, 0.0, 0.5),
        normal: Vec3::Y,
        over_terrain: true,
    });
    (app, tank, other)
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
    let (mut app, tank, other) = tank_floor_construction_fixture(TankEditKind::LowerFloor);
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
