use bevy::prelude::*;

use openzt2_zoo_state::economy::money_types::Money;
use openzt2_zoo_state::placement::placement_transaction_types::ObjectPlacementEditPreparationRejected;
use openzt2_zoo_state::placement::placement_transaction_types::ObjectPlacementEditPrepared;
use openzt2_zoo_state::terrain::terrain_edit_types::TerrainEditPreparationRejected;
use openzt2_zoo_state::terrain::terrain_edit_types::TerrainEditPrepared;
use openzt2_zoo_state::topology::topology_edit_types::TopologyEditPreparationRejected;
use openzt2_zoo_state::topology::topology_edit_types::TopologyEditPrepared;
use openzt2_zoo_state::transport_tours::transport_track_construction_types::TransportTrackConstructionEditPreparationRejected;
use openzt2_zoo_state::transport_tours::transport_track_construction_types::TransportTrackConstructionEditPrepared;

use super::*;
use openzt2_zoo_state::construction::construction_transaction_types::ConstructionCommitFailed;
use openzt2_zoo_state::construction::construction_transaction_types::EditCommitProgress;
use openzt2_zoo_state::construction::construction_transaction_types::EditTransaction;
use openzt2_zoo_state::construction::construction_transaction_types::TransactionState;

fn construction_preparation_test_application() -> App {
    let mut application = App::new();
    application
        .add_message::<openzt2_zoo_state::aquatic::tank_geometry_edit_types::TankGeometryEditPrepared>()
        .add_message::<openzt2_zoo_state::aquatic::tank_geometry_edit_types::TankGeometryEditPreparationRejected>()
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
