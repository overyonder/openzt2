use bevy::{ecs::system::SystemState, prelude::*};
use openzt2_game_data::AssetId;
use openzt2_world_state::world_spawn::{
    persistent_id_types::PersistentId,
    world_membership_types::{DefinitionId, WorldMember},
};
use openzt2_zoo_state::{
    economy::{
        guest_admission_types::{AdmissionPrice, ZooAdmissionsOpen},
        monthly_finance_types::MonthlyFinanceHistory,
        zoo_cash_types::ZooCash,
    },
    extinct_animals::fossil_recovery_types::FossilSiteKind,
};

use super::super::world_snapshot_capture_system_parameters::EconomyCaptureQueries;
use super::capture_economy_snapshot_records_from_live_world;

#[test]
fn fossil_sites_are_left_to_the_continuation_that_recreates_them() {
    let mut world = World::new();
    let root = world
        .spawn((
            ZooCash::default(),
            AdmissionPrice::default(),
            ZooAdmissionsOpen::default(),
            MonthlyFinanceHistory::default(),
        ))
        .id();
    let definition = DefinitionId(AssetId::from_key("fossil_site"));
    world.spawn((PersistentId(7), WorldMember { root }, definition));
    world.spawn((
        PersistentId(8),
        WorldMember { root },
        definition,
        FossilSiteKind::default(),
    ));
    let mut state = SystemState::<EconomyCaptureQueries>::new(&mut world);
    let records =
        capture_economy_snapshot_records_from_live_world(&state.get(&world).unwrap(), root)
            .unwrap();
    let saved: Vec<_> = records
        .entity_economy_records
        .iter()
        .map(|record| record.persistent_identifier)
        .collect();
    assert_eq!(saved, [PersistentId(7)]);
}
