use bevy::prelude::*;
use openzt2_game_data::AssetId;

use openzt2_game_assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use openzt2_zoo_state::construction::construction_transaction_types::PrepareDeletion;
use openzt2_world_state::world_spawn::persistent_id_types::PersistentId;

use crate::topology::topology_deletion_transaction_preparation::prepare_topology_deletion_transaction;
use crate::topology::topology_fence_definition_test_fixture::install_fence_definition_for_topology_transaction_tests;
use openzt2_zoo_state::topology::topology_edit_types::TopologyEdit;
use openzt2_zoo_state::topology::topology_edit_types::TopologyEditPreparationRejected;
use openzt2_zoo_state::topology::topology_edit_types::TopologyEditPrepared;
use openzt2_zoo_state::topology::topology_graph_types::FenceEdge;
use openzt2_zoo_state::topology::topology_graph_types::TopologyNode;

#[test]
fn deleting_a_node_prepares_its_incident_fence_without_mutation() {
    let mut app = App::new();
    app.init_resource::<Assets<WorldDefinitionAsset>>()
        .init_resource::<WorldDefinitions>()
        .add_message::<PrepareDeletion>()
        .add_message::<TopologyEditPrepared>()
        .add_message::<TopologyEditPreparationRejected>()
        .add_systems(Update, prepare_topology_deletion_transaction);
    install_fence_definition_for_topology_transaction_tests(&mut app, AssetId::default());
    let a = app
        .world_mut()
        .spawn((TopologyNode { cell: IVec3::ZERO }, PersistentId(1)))
        .id();
    let b = app
        .world_mut()
        .spawn((TopologyNode { cell: IVec3::X }, PersistentId(2)))
        .id();
    let fence = app
        .world_mut()
        .spawn((
            FenceEdge {
                definition: AssetId::default(),
                a,
                b,
            },
            PersistentId(3),
        ))
        .id();
    let transaction = app.world_mut().spawn_empty().id();
    app.world_mut().write_message(PrepareDeletion {
        transaction,
        target: a,
    });
    app.update();

    assert!(app.world().get_entity(a).is_ok());
    assert!(app.world().get_entity(fence).is_ok());
    let edit = app.world().get::<TopologyEdit>(transaction).unwrap();
    assert!(edit
        .removed
        .iter()
        .any(|snapshot| snapshot.id() == PersistentId(1)));
    assert!(edit
        .removed
        .iter()
        .any(|snapshot| snapshot.id() == PersistentId(3)));
}
