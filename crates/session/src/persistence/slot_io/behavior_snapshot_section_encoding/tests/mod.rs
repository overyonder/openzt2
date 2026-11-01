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
    let bytes = encode_behavior_actor_snapshots(&[record]).unwrap();
    let records = decode_behavior_actor_snapshots(
        &bytes,
        WorldSnapshotSectionDirectoryEntry {
            encoded_record_count: 1,
            payload_start_byte_offset: 0,
            payload_byte_count: bytes.len() as u64,
        },
    )
    .unwrap();
    let pending = records[0].transaction.as_ref().unwrap();
    assert_eq!(pending.transaction, AssetId::from_key("remove_user"));
    assert_eq!(pending.cost_override, Some(25.0_f32.to_bits()));
    assert_eq!(pending.origin.action, records[0].task.current.action);
    assert_eq!(
        pending.facility,
        BehaviorSnapshotEntityReference::Persistent(2)
    );
    let mut detached = records;
    detached[0].transaction.as_mut().unwrap().facility =
        BehaviorSnapshotEntityReference::Persistent(4);
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
    assert!(record.transaction.is_none());
    validate_behavior_actor_snapshots(&[record]).unwrap();
}
