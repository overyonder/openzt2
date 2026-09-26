use bevy::prelude::*;
use openzt2_game_data::AssetId;

use crate::plugins::{
    construction::construction_transaction_types::EditApplication,
    world_spawn::{
        persistent_id_types::PersistentId, world_membership_types::WorldMember,
        world_membership_types::WorldRoot,
    },
};

use super::{
    gate_operation_types::Gate,
    topology_edit_application::apply_committed_topology_edit,
    topology_edit_types::{
        CommitTopologyEdit, TopologyChanged, TopologyEdit, TopologyEditAcknowledged,
        TopologySnapshot,
    },
    topology_fence_definition_test_fixture::install_fence_definition_for_topology_transaction_tests,
    topology_graph_types::{TopologyGrid, TopologyIndex},
};
use crate::assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;

#[test]
fn undo_and_redo_restore_stable_identity_and_connectivity() {
    let mut app = App::new();
    app.insert_resource(Assets::<
        crate::assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset,
    >::default())
        .insert_resource(Assets::<
            crate::assets::terrain::terrain_asset_types_and_borrowing_queries::TerrainAsset,
        >::default())
        .init_resource::<WorldDefinitions>()
        .init_resource::<crate::plugins::terrain::terrain_chunk_types::TerrainIndex>()
        .init_resource::<TopologyIndex>()
        .init_resource::<TopologyGrid>()
        .add_message::<CommitTopologyEdit>()
        .add_message::<TopologyChanged>()
        .add_message::<TopologyEditAcknowledged>()
        .add_systems(Update, apply_committed_topology_edit);
    install_fence_definition_for_topology_transaction_tests(&mut app, AssetId::default());
    let root = app
        .world_mut()
        .spawn(WorldRoot {
            scenario: AssetId::default(),
        })
        .id();
    let edit = TopologyEdit {
        created: vec![
            TopologySnapshot::Node {
                id: PersistentId(100),
                cell: IVec3::ZERO,
            },
            TopologySnapshot::Node {
                id: PersistentId(101),
                cell: IVec3::X,
            },
            TopologySnapshot::Fence {
                id: PersistentId(102),
                definition: AssetId::default(),
                a: PersistentId(100),
                b: PersistentId(101),
                gate: Some(Gate {
                    open: true,
                    locked: false,
                }),
            },
        ]
        .into_boxed_slice(),
        removed: Box::new([]),
    };
    let transaction = app.world_mut().spawn((WorldMember { root }, edit)).id();

    for application in [
        EditApplication::InitialCommit,
        EditApplication::Undo,
        EditApplication::Redo,
    ] {
        app.world_mut().write_message(CommitTopologyEdit {
            transaction,
            application,
        });
        app.update();
        let index = app.world().resource::<TopologyIndex>();
        if application == EditApplication::Undo {
            assert!(index.nodes.is_empty());
            assert!(index.edges.is_empty());
        } else {
            assert_eq!(index.nodes.len(), 2);
            assert_eq!(index.edges.len(), 1);
        }
    }

    let mut ids = app.world_mut().query::<&PersistentId>();
    let mut values = ids
        .iter(app.world())
        .map(|id| id.0)
        .filter(|id| (100..=102).contains(id))
        .collect::<Vec<_>>();
    values.sort_unstable();
    assert_eq!(values, [100, 101, 102]);
}
