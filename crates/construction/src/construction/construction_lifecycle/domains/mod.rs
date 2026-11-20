use std::marker::PhantomData;

use bevy::{ecs::query::QueryItem, prelude::*};
use openzt2_zoo_state::{
    aquatic::tank_geometry_edit_types::*,
    construction::{
        construction_interaction_types::PlacementFailure,
        construction_transaction_types::{EditApplication, RollbackContext},
        ConstructionDomainAcknowledgementCollectionSet, ConstructionDomainApplicationDispatchSet,
        ConstructionDomainPreparationCollectionSet, ConstructionFailureRollbackDispatchSet,
        FixedConstructionPreparedDomainEditMaterializationSet, FixedConstructionSet,
    },
    economy::money_types::Money,
    placement::placement_transaction_types::*,
    shows::show_platform_upgrade_types::*,
    terrain::terrain_edit_types::*,
    topology::topology_edit_types::*,
    transport_tours::transport_track_construction_types::*,
};

use super::{ApplyEdit, ConstructionActive, Preparing};

/// A domain owns its edit data and its quoted contribution to the transfer.
#[derive(Component)]
pub struct PreparedDomain<D: Component>(pub Money, PhantomData<D>);

impl<D: Component> PreparedDomain<D> {
    pub fn new(cost: Money) -> Self {
        Self(cost, PhantomData)
    }
}

#[derive(Component)]
pub(super) struct PreparationRejected<D: Component>(PlacementFailure, PhantomData<D>);

#[derive(Component)]
pub(super) struct Applied<D: Component>(bool, PhantomData<D>);

#[derive(Component)]
pub(super) struct Reverted<D: Component>(bool, PhantomData<D>);

pub(super) type DomainFacts<D> = (
    Option<&'static PreparedDomain<D>>,
    Option<&'static PreparationRejected<D>>,
    Option<&'static Applied<D>>,
    Option<&'static Reverted<D>>,
);

pub(super) type Domains = (
    DomainFacts<TankGeometryEdit>,
    DomainFacts<TerrainEdit>,
    DomainFacts<TopologyEdit>,
    DomainFacts<PreparedObjectPlacementEdit>,
    DomainFacts<PreparedTransportTrackConstructionEdit>,
    DomainFacts<ShowPlatformUpgradeEdit>,
);

pub(super) type ApplicationResults = (
    Applied<TankGeometryEdit>,
    Applied<TerrainEdit>,
    Applied<TopologyEdit>,
    Applied<PreparedObjectPlacementEdit>,
    Applied<PreparedTransportTrackConstructionEdit>,
    Applied<ShowPlatformUpgradeEdit>,
    (
        Reverted<TankGeometryEdit>,
        Reverted<TerrainEdit>,
        Reverted<TopologyEdit>,
        Reverted<PreparedObjectPlacementEdit>,
        Reverted<PreparedTransportTrackConstructionEdit>,
        Reverted<ShowPlatformUpgradeEdit>,
    ),
);

trait Domain: Component {
    type Request: Message;
    type Result: Message;

    fn request(transaction: Entity, application: EditApplication) -> Self::Request;
    fn result(result: &Self::Result) -> (Entity, EditApplication, bool);
}

trait PreparesDomain: Domain {
    type Prepared: Message;
    type Rejected: Message;

    fn prepared(result: &Self::Prepared) -> (Entity, Money);
    fn rejected(result: &Self::Rejected) -> (Entity, PlacementFailure);
}

macro_rules! domain_messages {
    ($edit:ty, $request:ident, $result:ty $(, $prepared:ty, $rejected:ty)?) => {
        impl Domain for $edit {
            type Request = $request;
            type Result = $result;

            fn request(transaction: Entity, application: EditApplication) -> Self::Request {
                $request { transaction, application }
            }

            fn result(result: &Self::Result) -> (Entity, EditApplication, bool) {
                (result.transaction, result.application, result.accepted)
            }
        }

        $(
            impl PreparesDomain for $edit {
                type Prepared = $prepared;
                type Rejected = $rejected;

                fn prepared(result: &Self::Prepared) -> (Entity, Money) {
                    (result.transaction, result.cost)
                }

                fn rejected(result: &Self::Rejected) -> (Entity, PlacementFailure) {
                    (result.transaction, result.reason)
                }
            }
        )?
    };
}

domain_messages!(
    TankGeometryEdit,
    CommitTankGeometryEdit,
    TankGeometryEditAcknowledged,
    TankGeometryEditPrepared,
    TankGeometryEditPreparationRejected
);
domain_messages!(
    TerrainEdit,
    CommitTerrainEdit,
    TerrainEditAcknowledged,
    TerrainEditPrepared,
    TerrainEditPreparationRejected
);
domain_messages!(
    TopologyEdit,
    CommitTopologyEdit,
    TopologyEditAcknowledged,
    TopologyEditPrepared,
    TopologyEditPreparationRejected
);
domain_messages!(
    PreparedObjectPlacementEdit,
    ApplyPreparedObjectPlacementEditRequest,
    ObjectPlacementEditApplicationAcknowledged,
    ObjectPlacementEditPrepared,
    ObjectPlacementEditPreparationRejected
);
domain_messages!(
    PreparedTransportTrackConstructionEdit,
    ApplyPreparedTransportTrackConstructionEditRequest,
    TransportTrackConstructionEditApplicationAcknowledged,
    TransportTrackConstructionEditPrepared,
    TransportTrackConstructionEditPreparationRejected
);
domain_messages!(
    ShowPlatformUpgradeEdit,
    CommitShowPlatformUpgradeEdit,
    ShowPlatformUpgradeEditAcknowledged
);

fn collect_preparation<D: PreparesDomain>(
    mut commands: Commands,
    mut prepared: MessageReader<D::Prepared>,
    mut rejected: MessageReader<D::Rejected>,
    preparing: Query<(), With<Preparing>>,
) {
    // A domain contributes once even if a producer repeats a notification.
    let mut quoted = bevy::ecs::entity::EntityHashSet::new();
    for (entity, cost) in prepared.read().map(D::prepared) {
        if preparing.contains(entity) && quoted.insert(entity) {
            commands
                .entity(entity)
                .insert(PreparedDomain::<D>::new(cost));
        }
    }
    quoted.clear();
    for (entity, reason) in rejected.read().map(D::rejected) {
        if preparing.contains(entity) && quoted.insert(entity) {
            commands
                .entity(entity)
                .insert(PreparationRejected::<D>(reason, PhantomData));
        }
    }
}

fn dispatch<D: Domain>(
    edits: Query<(Entity, &ApplyEdit), (With<D>, With<PreparedDomain<D>>, Added<ApplyEdit>)>,
    mut requests: MessageWriter<D::Request>,
) {
    for (entity, application) in &edits {
        requests.write(D::request(entity, application.0));
    }
}

fn dispatch_rollback<D: Domain>(
    edits: Query<(Entity, &Applied<D>), (With<D>, Added<RollbackContext>)>,
    mut requests: MessageWriter<D::Request>,
) {
    for (entity, applied) in &edits {
        if applied.0 {
            requests.write(D::request(entity, EditApplication::FailureRollback));
        }
    }
}

fn collect_result<D: Domain>(
    mut commands: Commands,
    mut results: MessageReader<D::Result>,
    edits: Query<
        (
            Option<&ApplyEdit>,
            Option<&RollbackContext>,
            Option<&Applied<D>>,
            Has<Reverted<D>>,
        ),
        (With<ConstructionActive>, With<PreparedDomain<D>>),
    >,
) {
    let mut received = bevy::ecs::entity::EntityHashSet::new();
    for (entity, application, accepted) in results.read().map(D::result) {
        let Ok((apply, rollback, applied, reverted)) = edits.get(entity) else {
            continue;
        };
        if application == EditApplication::FailureRollback {
            if rollback.is_some()
                && applied.is_some_and(|result| result.0)
                && !reverted
                && received.insert(entity)
            {
                commands
                    .entity(entity)
                    .insert(Reverted::<D>(accepted, PhantomData));
            }
        } else if apply.is_some_and(|apply| apply.0 == application)
            && applied.is_none()
            && received.insert(entity)
        {
            commands
                .entity(entity)
                .insert(Applied::<D>(accepted, PhantomData));
        }
    }
}

pub(super) fn install(app: &mut App) {
    install_domain::<TankGeometryEdit>(app);
    install_domain::<TerrainEdit>(app);
    install_domain::<TopologyEdit>(app);
    install_domain::<PreparedObjectPlacementEdit>(app);
    install_domain::<PreparedTransportTrackConstructionEdit>(app);
    install_application::<ShowPlatformUpgradeEdit>(app);
}

fn install_domain<D: PreparesDomain>(app: &mut App) {
    install_application::<D>(app);
    app.add_message::<D::Prepared>()
        .add_message::<D::Rejected>()
        .add_systems(
            FixedUpdate,
            collect_preparation::<D>
                .after(FixedConstructionPreparedDomainEditMaterializationSet)
                .in_set(ConstructionDomainPreparationCollectionSet)
                .in_set(FixedConstructionSet::Prepare),
        );
}

fn install_application<D: Domain>(app: &mut App) {
    app.add_message::<D::Request>()
        .add_message::<D::Result>()
        .add_systems(
            FixedUpdate,
            (
                dispatch::<D>.in_set(ConstructionDomainApplicationDispatchSet),
                dispatch_rollback::<D>.in_set(ConstructionFailureRollbackDispatchSet),
            )
                .in_set(FixedConstructionSet::Apply),
        )
        .add_systems(
            FixedUpdate,
            collect_result::<D>
                .in_set(ConstructionDomainAcknowledgementCollectionSet)
                .in_set(FixedConstructionSet::Cleanup),
        );
}

#[derive(Clone, Copy)]
pub(super) struct SettlementFacts {
    pub cost: Option<Money>,
    pub rejection: Option<PlacementFailure>,
    pub accepted: Option<bool>,
    pub reverted: Option<bool>,
}

fn facts<D: Component>(
    (prepared, rejected, applied, reverted): (
        Option<&PreparedDomain<D>>,
        Option<&PreparationRejected<D>>,
        Option<&Applied<D>>,
        Option<&Reverted<D>>,
    ),
) -> SettlementFacts {
    SettlementFacts {
        cost: prepared.map(|prepared| prepared.0),
        rejection: rejected.map(|rejected| rejected.0),
        accepted: applied.map(|applied| applied.0),
        reverted: reverted.map(|reverted| reverted.0),
    }
}

pub(super) fn settlement_facts(
    (tank, terrain, topology, placement, track, shows): QueryItem<Domains>,
) -> [SettlementFacts; 6] {
    [
        facts(tank),
        facts(terrain),
        facts(topology),
        facts(placement),
        facts(track),
        facts(shows),
    ]
}
