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
        evaluation_interval_seconds: 1.0,
        id: definition,
        name_key: definition,
        description_key: definition,
        time_limit_ticks: 0,
        flags: ScenarioRecordFlags::default(),
        ui_modifications: vec![],
        objective_groups: vec![],
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
                update_when_paused: false,
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
    source.entity_mut(root).insert(ScenarioScriptTiming {
        evaluation_elapsed_seconds: 2.25,
        always_run_elapsed_seconds: 0.75,
    });
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
    let legacy = postcard::to_allocvec(&saved).unwrap();
    let legacy_record = decode(
        &legacy,
        WorldSnapshotSectionDirectoryEntry {
            encoded_record_count: record_count(Some(&saved)),
            payload_start_byte_offset: 0,
            payload_byte_count: legacy.len() as u64,
        },
    )
    .unwrap()
    .unwrap();
    assert!(legacy_record.timing.is_none());
    let mut invalid = bytes.clone();
    let end = invalid.len();
    invalid[end - 4..].copy_from_slice(&f32::NAN.to_le_bytes());
    assert!(decode(
        &invalid,
        WorldSnapshotSectionDirectoryEntry {
            encoded_record_count: record_count(Some(&saved)),
            payload_start_byte_offset: 0,
            payload_byte_count: invalid.len() as u64
        }
    )
    .is_err());

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
    let retained_handle = destination
        .resource::<WorldScenarios>()
        .get(destination.resource::<Assets<WorldScenarioDocumentAsset>>())
        .unwrap()
        .handle_for_scenario(definition)
        .unwrap()
        .clone();
    let mut stock_document = WorldScenarioDocument::default();
    let mut stock_definition = destination
        .resource::<Assets<WorldScenarioDocumentAsset>>()
        .get(&retained_handle)
        .unwrap()
        .document
        .scenarios[0]
        .clone();
    stock_definition.id = AssetId::from_key("other-stock-scenario");
    stock_document.scenarios.push(stock_definition);
    let stock_handle = destination
        .resource_mut::<Assets<WorldScenarioDocumentAsset>>()
        .add(WorldScenarioDocumentAsset::from_test_document(
            stock_document.clone(),
        ));
    let mut catalogue = WorldScenarios::default();
    catalogue.index_test_document(stock_handle, &stock_document);
    destination.insert_resource(catalogue);
    destination
        .entity_mut(restored_root)
        .insert(SelectedWorldDocumentHandles {
            map: retained_handle.clone(),
            start: retained_handle,
        });
    destination.insert_resource(Messages::<ScenarioObjectiveStatusChanged>::default());
    let mut apply_state = SystemState::<(
        Commands,
        ResMut<PersistentIdAllocator>,
        ScenarioSnapshotApplication,
    )>::new(&mut destination);
    {
        let (mut commands, mut allocator, mut parameters) =
            apply_state.get_mut(&mut destination).unwrap();
        let prepared = prepare(decoded, definition, restored_root, &parameters).unwrap();
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

#[test]
fn original_objective_planning_preserves_baseline_ids_and_applies_only_after_validation() {
    use super::original_saved_game_conversion::assign_original_scenario_objective_identifiers;

    let (mut world, root, definition, rules) = initialize_world();
    let baseline = world.spawn((PersistentId(9), WorldMember { root })).id();
    world.spawn((PersistentId(10), WorldMember { root }));
    world.insert_resource(PersistentIdAllocator::from_snapshot(root, 503).unwrap());
    let source = ScenarioSnapshot {
        timing: Some(ScenarioScriptTiming {
            evaluation_elapsed_seconds: 2.0,
            always_run_elapsed_seconds: 0.5,
        }),
        version: 1,
        definition,
        started_tick: 318,
        state: 0,
        objectives: rules
            .iter()
            .enumerate()
            .map(|(index, rule)| ScenarioObjectiveSnapshot {
                id: 9 + index as u64,
                definition: *rule,
                status: 1,
                progress: [index as i64, 7],
                deadline: Some(900),
                visible: index == 0,
            })
            .collect(),
        lua: None,
        ui: Default::default(),
    };
    let mut planned = Some(source.clone());
    let next = assign_original_scenario_objective_identifiers(
        &mut planned,
        root,
        world.resource::<PersistentIdAllocator>(),
    )
    .unwrap();
    assert_eq!(next, 505);
    assert_eq!(world.resource::<PersistentIdAllocator>().next(), 503);
    let mut repeated = Some(source.clone());
    assert_eq!(
        assign_original_scenario_objective_identifiers(
            &mut repeated,
            root,
            world.resource::<PersistentIdAllocator>()
        )
        .unwrap(),
        next
    );
    assert_eq!(repeated, planned);

    let mut exhausted = Some(source.clone());
    let exhausted_allocator = PersistentIdAllocator::from_snapshot(root, u64::MAX - 1).unwrap();
    assert!(assign_original_scenario_objective_identifiers(
        &mut exhausted,
        root,
        &exhausted_allocator
    )
    .is_err());
    assert_eq!(exhausted, Some(source));
    assert_eq!(exhausted_allocator.next(), u64::MAX - 1);

    let expected = planned.clone();
    let mut application = SystemState::<(
        Commands,
        ResMut<PersistentIdAllocator>,
        ScenarioSnapshotApplication,
    )>::new(&mut world);
    {
        let (mut commands, mut allocator, mut parameters) =
            application.get_mut(&mut world).unwrap();
        let prepared = prepare(planned, definition, root, &parameters).unwrap();
        apply(
            &mut commands,
            prepared,
            root,
            &mut allocator,
            &mut parameters,
        )
        .unwrap();
    }
    application.apply(&mut world);
    assert_eq!(world.get::<PersistentId>(baseline), Some(&PersistentId(9)));
    assert_eq!(world.resource::<PersistentIdAllocator>().next(), next);
    let mut capture_state = SystemState::<ScenarioSnapshotCapture>::new(&mut world);
    assert_eq!(
        capture(&capture_state.get(&world).unwrap()).unwrap(),
        expected
    );
}
