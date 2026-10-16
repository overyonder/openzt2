use super::*;
use bevy::ecs::system::SystemState;
use openzt2_game_data::world_scenario::{
    ScenarioDefinitionRecord, ScenarioObjectiveRecord, ScenarioRecordFlags, WorldScenarioDocument,
};

fn initialize_world() -> (World, Entity, AssetId, [AssetId; 2]) {
    let mut world = World::new();
    let definition = AssetId::from_key("saved-scenario");
    let rules = [
        AssetId::from_key("first-rule"),
        AssetId::from_key("second-rule"),
    ];
    let mut document = WorldScenarioDocument::default();
    document.scenarios.push(ScenarioDefinitionRecord {
        id: definition,
        name_key: definition,
        description_key: definition,
        time_limit_ticks: 0,
        flags: ScenarioRecordFlags::default(),
        ui_modifications: vec![],
        objectives: rules
            .iter()
            .enumerate()
            .map(|(index, id)| ScenarioObjectiveRecord {
                id: *id,
                name: None,
                category: definition,
                text_key: definition,
                success_text_key: definition,
                failure_text_key: definition,
                hidden: false,
                prerequisite_objectives: if index == 0 { vec![] } else { vec![rules[0]] },
            })
            .collect(),
    });
    let mut assets = Assets::<WorldScenarioDocumentAsset>::default();
    let handle = assets.add(WorldScenarioDocumentAsset::from_test_document(
        document.clone(),
    ));
    let mut scenarios = WorldScenarios::default();
    scenarios.index_test_document(handle, &document);
    world.insert_resource(assets);
    world.insert_resource(scenarios);
    world.insert_non_send(ScenarioLuaVirtualMachineContexts::default());
    let root = world
        .spawn(WorldRoot {
            scenario: definition,
        })
        .id();
    world.insert_resource(PersistentIdAllocator::new(root));
    (world, root, definition, rules)
}

#[test]
fn campaign_state_round_trip_keeps_completed_rules_and_live_deadlines() {
    let (mut source, root, definition, _) = initialize_world();
    source.entity_mut(root).insert(ActiveScenarioSession {
        definition,
        started_tick: 41,
        state: ScenarioSessionState::Running,
    });
    for (index, status) in [
        ScenarioObjectiveStatus::Satisfied,
        ScenarioObjectiveStatus::Active,
    ]
    .into_iter()
    .enumerate()
    {
        source.spawn((
            PersistentId(100 + index as u64),
            WorldMember { root },
            ScenarioObjective {
                scenario: definition,
                record_index: index as u32,
            },
            status,
            ScenarioObjectiveProgress {
                current: index as i64 + 2,
                target: 7,
            },
            ScenarioObjectiveDeadline { end_tick: 900 },
            ScenarioObjectiveAuthoredVisibility(index != 0),
        ));
    }
    let mut capture_state = SystemState::<ScenarioSnapshotCapture>::new(&mut source);
    let saved = capture(&capture_state.get(&source).unwrap())
        .unwrap()
        .unwrap();
    let bytes = encode(Some(&saved)).unwrap();
    let decoded = decode(
        &bytes,
        WorldSnapshotSectionDirectoryEntry {
            encoded_record_count: record_count(Some(&saved)),
            payload_start_byte_offset: 0,
            payload_byte_count: bytes.len() as u64,
        },
    )
    .unwrap();
    assert!(decode(&[], WorldSnapshotSectionDirectoryEntry::default())
        .unwrap()
        .is_none());
    let (mut destination, restored_root, _, _) = initialize_world();
    destination.insert_resource(Messages::<ScenarioObjectiveStatusChanged>::default());
    let mut apply_state = SystemState::<(
        Commands,
        ResMut<PersistentIdAllocator>,
        ScenarioSnapshotApplication,
    )>::new(&mut destination);
    {
        let (mut commands, mut allocator, mut parameters) =
            apply_state.get_mut(&mut destination).unwrap();
        let prepared = prepare(decoded, definition, &parameters).unwrap();
        apply(
            &mut commands,
            prepared,
            restored_root,
            &mut allocator,
            &mut parameters,
        )
        .unwrap();
    }
    apply_state.apply(&mut destination);
    let mut capture_state = SystemState::<ScenarioSnapshotCapture>::new(&mut destination);
    assert_eq!(
        capture(&capture_state.get(&destination).unwrap()).unwrap(),
        Some(saved)
    );
    assert!(destination
        .resource::<Messages<ScenarioObjectiveStatusChanged>>()
        .is_empty());
    let mut objectives = destination.query::<(
        Entity,
        &ScenarioObjective,
        &ScenarioObjectivePrerequisiteEntities,
    )>();
    let first = objectives
        .iter(&destination)
        .find(|(_, record, _)| record.record_index == 0)
        .unwrap()
        .0;
    let second = objectives
        .iter(&destination)
        .find(|(_, record, _)| record.record_index == 1)
        .unwrap();
    assert_eq!(&*second.2 .0, &[first]);
}
