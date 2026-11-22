use bevy::prelude::*;
use openzt2_game_data::AssetId;

use openzt2_world_state::world_spawn::persistent_id_index::PersistentIdIndex;
use openzt2_world_state::world_spawn::persistent_id_types::PersistentId;
use openzt2_world_state::world_spawn::world_membership_types::WorldMember;
use openzt2_world_state::world_spawn::world_membership_types::WorldRoot;
use openzt2_zoo_state::construction::construction_transaction_types::EditApplication;
use openzt2_zoo_state::topology::topology_graph_types::Portal;

use openzt2_zoo_state::topology::gate_operation_types::Gate;
use crate::topology::topology_edit_application::apply_committed_topology_edit;
use openzt2_zoo_state::topology::topology_edit_types::CommitTopologyEdit;
use openzt2_zoo_state::topology::topology_edit_types::TopologyChanged;
use openzt2_zoo_state::topology::topology_edit_types::TopologyEdit;
use openzt2_zoo_state::topology::topology_edit_types::TopologyEditAcknowledged;
use openzt2_zoo_state::topology::topology_edit_types::TopologySnapshot;
use crate::topology::topology_fence_definition_test_fixture::install_fence_definition_for_topology_transaction_tests;
use openzt2_zoo_state::topology::topology_graph_types::TopologyGrid;
use openzt2_zoo_state::topology::topology_graph_types::TopologyIndex;
use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;

#[test]
fn undo_and_redo_restore_stable_identity_and_connectivity() {
    let mut app = App::new();
    app.add_plugins(openzt2_world_state::world_spawn::persistent_id_index::PersistentIdIndexPlugin);
    app.insert_resource(Assets::<
        openzt2_game_assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset,
    >::default())
        .insert_resource(Assets::<
            openzt2_game_assets::terrain::terrain_asset_types_and_borrowing_queries::TerrainAsset,
        >::default())
        .init_resource::<WorldDefinitions>()
        .add_message::<CommitTopologyEdit>()
        .add_message::<TopologyChanged>()
        .add_message::<TopologyEditAcknowledged>()
        .add_systems(Update, apply_committed_topology_edit);
    install_fence_definition_for_topology_transaction_tests(&mut app, AssetId::default());
    let root = app
        .world_mut()
        .spawn((
            WorldRoot {
                scenario: AssetId::default(),
            },
            openzt2_zoo_state::terrain::terrain_chunk_types::TerrainIndex::default(),
            TopologyIndex::default(),
            TopologyGrid::default(),
        ))
        .id();
    let entrance_pose = Transform::from_xyz(2.0, 3.0, -4.0);
    let exit_pose = Transform::from_xyz(7.0, 8.0, -9.0);
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
                wall_top: None,
            },
            TopologySnapshot::Portal {
                id: PersistentId(103),
                destination: PersistentId(104),
                bidirectional: false,
                endpoint_pose: Some(entrance_pose),
            },
            TopologySnapshot::Portal {
                id: PersistentId(104),
                destination: PersistentId(101),
                bidirectional: false,
                endpoint_pose: Some(exit_pose),
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
        let world = app.world();
        let index = world.get::<TopologyIndex>(root).unwrap();
        let ids = world.resource::<PersistentIdIndex>();
        let entrance = ids.get(root, PersistentId(103));
        let exit = ids.get(root, PersistentId(104));
        if application == EditApplication::Undo {
            assert!(index.nodes.is_empty());
            assert!(index.edges.is_empty());
            assert!(entrance.is_none() && exit.is_none());
        } else {
            assert_eq!(index.nodes.len(), 2);
            assert_eq!(index.edges.len(), 1);
            let (entrance, exit) = (entrance.unwrap(), exit.unwrap());
            assert_eq!(world.get::<Portal>(entrance).unwrap().destination, exit);
            assert_eq!(
                world.get::<GlobalTransform>(entrance),
                Some(&GlobalTransform::from(entrance_pose))
            );
            assert_eq!(
                world.get::<GlobalTransform>(exit),
                Some(&GlobalTransform::from(exit_pose))
            );
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
