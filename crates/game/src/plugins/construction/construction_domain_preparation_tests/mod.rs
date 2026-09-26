use bevy::prelude::*;

use crate::plugins::{
    economy::money_types::Money,
    placement::placement_transaction_types::{
        ObjectPlacementEditPreparationRejected, ObjectPlacementEditPrepared,
    },
    terrain::terrain_edit_types::{TerrainEditPreparationRejected, TerrainEditPrepared},
    topology::topology_edit_types::{TopologyEditPreparationRejected, TopologyEditPrepared},
    transport_tours::transport_track_construction_types::{
        TransportTrackConstructionEditPreparationRejected, TransportTrackConstructionEditPrepared,
    },
};

use super::construction_transaction_types::{
    ConstructionCommitFailed, EditCommitProgress, EditTransaction, TransactionState,
};
use super::*;

fn construction_preparation_test_application() -> App {
    let mut application = App::new();
    application
        .add_message::<TerrainEditPrepared>()
        .add_message::<TerrainEditPreparationRejected>()
        .add_message::<TopologyEditPrepared>()
        .add_message::<TopologyEditPreparationRejected>()
        .add_message::<ObjectPlacementEditPrepared>()
        .add_message::<ObjectPlacementEditPreparationRejected>()
        .add_message::<TransportTrackConstructionEditPrepared>()
        .add_message::<TransportTrackConstructionEditPreparationRejected>()
        .add_message::<ConstructionCommitFailed>()
        .add_systems(
            Update,
            construction_domain_preparation_collection::collect_construction_domain_preparation_results,
        );
    application
}

fn spawn_preparing_construction_transaction(application: &mut App) -> Entity {
    application
        .world_mut()
        .spawn((
            EditTransaction {
                sequence: 1,
                state: TransactionState::Applied,
                cost: Money(0),
            },
            EditCommitProgress::preparing(),
        ))
        .id()
}

#[test]
fn multi_domain_cost_overflow_rejects_before_economy() {
    let mut application = construction_preparation_test_application();
    let transaction_entity = spawn_preparing_construction_transaction(&mut application);
    application.world_mut().write_message(TerrainEditPrepared {
        transaction: transaction_entity,
        cost: Money(i64::MAX),
    });
    application.world_mut().write_message(TopologyEditPrepared {
        transaction: transaction_entity,
        cost: Money(1),
    });
    application.update();
    assert!(application.world().get_entity(transaction_entity).is_err());
}
