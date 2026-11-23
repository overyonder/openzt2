use super::*;
use openzt2_game_data::AssetId;

fn transaction_record() -> BehaviorActorSnapshot {
    serde_json::from_value(serde_json::json!({
        "persistent_identifier": 1,
        "task": {
            "execution_id": 3,
            "current": { "document": "AI/Tasks/staff/Keeper.tsk", "program": AssetId::from_key("keeper:FillFoodContainer"),
                "subjects": [], "phase": "Execution", "action": 8, "repetitions": 0 },
            "target": { "Persistent": 2 }, "object": null, "next_action_tick": 17,
            "interaction_slot": null, "stack": [], "score": null, "failing": false
        },
        "navigation": null, "water_target": null, "staff_job": null,
        "random": null, "synchronized": null, "membership": null,
        "transaction": {
            "execution_id": 3, "stack_depth": 0,
            "origin": { "document": "AI/Tasks/staff/Keeper.tsk", "program": AssetId::from_key("keeper:FillFoodContainer"),
                "subjects": [], "phase": "Execution", "action": 8, "repetitions": 0 },
            "facility": { "Persistent": 2 }, "transaction": AssetId::from_key("remove_user"),
            "cost_override": 1103626240u32, "next_transaction": null
        }
    })).unwrap()
}

#[test]
fn named_transaction_continuation_survives_section_round_trip() {
    let record = transaction_record();
    let mut another_actor = transaction_record();
    let BehaviorActorSnapshot::Task(other) = &mut another_actor else {
        panic!("expected task record");
    };
    other.persistent_identifier = 3;
    let bytes = encode_behavior_actor_snapshots(&[record, another_actor]).unwrap();
    let records = decode_behavior_actor_snapshots(
        &bytes,
        WorldSnapshotSectionDirectoryEntry {
            encoded_record_count: 2,
            payload_start_byte_offset: 0,
            payload_byte_count: bytes.len() as u64,
        },
    )
    .unwrap();
    assert_eq!(
        records[0].task_record().unwrap().task.execution_id,
        records[1].task_record().unwrap().task.execution_id,
    );
    let pending = records[0]
        .task_record()
        .unwrap()
        .transaction
        .as_ref()
        .unwrap();
    assert_eq!(pending.transaction, AssetId::from_key("remove_user"));
    assert_eq!(pending.cost_override, Some(25.0_f32.to_bits()));
    assert_eq!(
        pending.origin.action,
        records[0].task_record().unwrap().task.current.action
    );
    assert_eq!(
        pending.facility,
        BehaviorSnapshotEntityReference::Persistent(2)
    );
    let mut detached = records;
    let BehaviorActorSnapshot::Task(record) = &mut detached[0] else {
        panic!("expected task record");
    };
    record.transaction.as_mut().unwrap().facility = BehaviorSnapshotEntityReference::Persistent(4);
    assert_eq!(
        validate_behavior_actor_snapshots(&detached),
        Err(WorldSnapshotPersistenceFailure::CorruptSnapshotSection)
    );
}

#[test]
fn earlier_behavior_records_without_transaction_remain_readable() {
    let mut value = serde_json::to_value(transaction_record()).unwrap();
    value.as_object_mut().unwrap().remove("transaction");
    let record: BehaviorActorSnapshot = serde_json::from_value(value).unwrap();
    assert!(record.task_record().unwrap().transaction.is_none());
    validate_behavior_actor_snapshots(&[record]).unwrap();
}

#[test]
fn actor_owned_navigation_ids_round_trip_for_two_actors() {
    use super::super::behavior_snapshot_types::BehaviorOwnedNavigationSnapshot;
    use openzt2_zoo_state::locomotion::locomotion_types::{
        Destination, NavAgent, NavFlags, Route, Velocity,
    };

    let mut records = [transaction_record(), transaction_record()];
    for (index, record) in records.iter_mut().enumerate() {
        let BehaviorActorSnapshot::Task(record) = record else {
            panic!("expected task record");
        };
        record.persistent_identifier = 1 + index as u64 * 2;
        record.transaction = None;
        record.navigation = Some(BehaviorOwnedNavigationSnapshot {
            agent: NavAgent {
                radius_m: 0.4,
                max_speed_mps: 1.0,
                acceleration_mps2: 2.0,
                capabilities: NavFlags::ANIMAL,
            },
            mode: Default::default(),
            swim_layer: None,
            water_contact: None,
            velocity: Velocity::default(),
            contact_steering: None,
            progress: None,
            destination: Some(Destination {
                request_id: 1,
                world: bevy::prelude::Vec3::X,
                arrival_radius_m: 0.4,
            }),
            route: Route::with_capacity(0),
            pursuit_target: None,
            docking: None,
            move_completion: None,
            docking_completion: None,
            pursuit: None,
            follow: Some(
                super::super::behavior_snapshot_types::BehaviorFollowSnapshot {
                    target: BehaviorSnapshotEntityReference::Persistent(2),
                    until_tick: 22,
                    request_id: 1,
                },
            ),
            spline: None,
            pending_results: Vec::new(),
        });
    }
    let bytes = encode_behavior_actor_snapshots(&records).unwrap();
    let records = decode_behavior_actor_snapshots(
        &bytes,
        WorldSnapshotSectionDirectoryEntry {
            encoded_record_count: 2,
            payload_start_byte_offset: 0,
            payload_byte_count: bytes.len() as u64,
        },
    )
    .unwrap();
    for record in &records {
        let follow = record
            .task_record()
            .unwrap()
            .navigation
            .as_ref()
            .unwrap()
            .follow
            .as_ref()
            .unwrap();
        assert_eq!(follow.request_id, 1);
        assert_eq!(follow.until_tick, 22);
    }
}
