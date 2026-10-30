use super::{
    super::{
        snapshot_container_encoding_and_validation::*,
        snapshot_container_types::WorldSnapshotSectionKind,
    },
    maintenance_snapshot_application::apply_loose_waste_records,
    maintenance_snapshot_capture::{capture_loose_waste_records, LooseWasteSnapshotEntities},
    maintenance_snapshot_section_encoding::{
        decode_maintenance_snapshot, encode_maintenance_snapshot,
    },
    maintenance_snapshot_types::{
        LooseWasteSnapshotRecord, MaintainedObjectSnapshotRecord, MaintenanceSnapshot,
    },
};
use bevy::{ecs::system::SystemState, prelude::*};
use openzt2_game_assets::{
    behavior::behavior_asset_types::BehaviorDocumentAsset,
    scene_prefab::ScenePrefabAsset,
    world_definitions::{
        world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions,
        world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset,
    },
};
use openzt2_game_data::{world_definitions::staff_management::StaffJobKind, AssetId};
use openzt2_world_state::simulation_time::simulation_clock_types::ZooClock;
use openzt2_world_state::world_spawn::persistent_id_types::PersistentId;
use openzt2_world_state::world_spawn::persistent_id_types::PersistentIdAllocator;
use openzt2_world_state::world_spawn::world_membership_types::DefinitionId;
use openzt2_world_state::world_spawn::world_membership_types::WorldMember;
use openzt2_zoo_staff::maintenance::add_animal_waste_cleanup_test_systems;
use openzt2_zoo_state::behavior_task_execution_types::BehaviorTaskExecutionPhase;
use openzt2_zoo_state::behavior_task_execution_types::BehaviorTaskExecutionState;
use openzt2_zoo_state::behavior_task_execution_types::BehaviorTaskFailed;
use openzt2_zoo_state::behavior_task_execution_types::BehaviorTaskFinished;
use openzt2_zoo_state::maintenance::maintenance_types::AnimalHabitatWaste;
use openzt2_zoo_state::maintenance::maintenance_types::IncrementalZooCleanlinessTotals;
use openzt2_zoo_state::staff::staff_employment_types::Staff;
use openzt2_zoo_state::staff::staff_job_types::CurrentJob;
use openzt2_zoo_state::staff::staff_job_types::JobClaim;
use openzt2_zoo_state::staff::staff_job_types::StaffJob;
use openzt2_zoo_state::staff::staff_job_types::StaffJobBehaviorTask;
use openzt2_zoo_state::staff::staff_lifecycle_messages::CancelStaffJobRequest;
use openzt2_zoo_state::staff::staff_lifecycle_messages::StaffJobCompleted;
use openzt2_zoo_state::staff::staff_lifecycle_messages::StaffJobRequest;

fn fixture() -> (App, Entity) {
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, AssetPlugin::default()))
        .init_resource::<Assets<ScenePrefabAsset>>()
        .init_resource::<Assets<BehaviorDocumentAsset>>()
        .init_resource::<Assets<WorldDefinitionAsset>>()
        .init_resource::<WorldDefinitions>()
        .insert_resource(ZooClock {
            tick: 90,
            absolute_day: 0,
            tick_in_day: 90,
        })
        .add_message::<StaffJobRequest>()
        .add_message::<StaffJobCompleted>()
        .add_message::<CancelStaffJobRequest>()
        .add_message::<BehaviorTaskFinished>()
        .add_message::<BehaviorTaskFailed>();
    use openzt2_game_data::world_definitions::{
        document::WorldDefinitionDocument,
        staff_management::{
            StaffRequestControllerDefinition, StaffRequestData, StaffRequestThresholdComparison,
            StaffRequestThresholdValue,
        },
    };
    let document = WorldDefinitionDocument {
        staff_requests: vec![StaffRequestControllerDefinition {
            id: AssetId::from_key("test_waste"),
            binder: None,
            attribute_key: None,
            threshold: StaffRequestThresholdValue::Number(0.0),
            threshold_comparison: StaffRequestThresholdComparison::ReachesOrAbove,
            cancel_threshold: StaffRequestThresholdValue::Number(0.0),
            cancel_comparison: StaffRequestThresholdComparison::FallsBelow,
            trigger_on_creation: true,
            trigger_on_destruction: false,
            request: StaffRequestData {
                token: Some(AssetId::from_key("t_rakepoo")),
                ..StaffRequestData::CONSTRUCTED_DEFAULT
            },
        }],
        ..Default::default()
    };
    let handle = app
        .world_mut()
        .resource_mut::<Assets<WorldDefinitionAsset>>()
        .add(WorldDefinitionAsset::from_test_document(document.clone()));
    app.world_mut()
        .resource_mut::<WorldDefinitions>()
        .index_test_document(handle, &document);
    add_animal_waste_cleanup_test_systems(&mut app);
    let root = app.world_mut().spawn_empty().id();
    app.insert_resource(PersistentIdAllocator::new(root));
    (app, root)
}

fn capture(world: &mut World) -> Vec<LooseWasteSnapshotRecord> {
    let mut state = SystemState::<LooseWasteSnapshotEntities>::new(world);
    capture_loose_waste_records(&state.get(world).unwrap()).unwrap()
}

fn restore(
    app: &mut App,
    root: Entity,
    records: &[LooseWasteSnapshotRecord],
    prefabs: &[Option<Handle<ScenePrefabAsset>>],
) {
    let mut state = SystemState::<(
        Commands,
        ResMut<PersistentIdAllocator>,
        Res<Assets<ScenePrefabAsset>>,
        Query<Entity, super::maintenance_snapshot_capture::LooseWasteSnapshotFilter>,
    )>::new(app.world_mut());
    {
        let (mut commands, mut ids, assets, baseline) = state.get_mut(app.world_mut()).unwrap();
        apply_loose_waste_records(
            &mut commands,
            records,
            prefabs,
            &assets,
            root,
            &mut ids,
            &baseline,
        )
        .unwrap();
    }
    state.apply(app.world_mut());
}

fn round_trip(records: &[LooseWasteSnapshotRecord]) -> Vec<LooseWasteSnapshotRecord> {
    let snapshot = MaintenanceSnapshot {
        waste: records.to_vec(),
        objects: Vec::new(),
    };
    decode_section(
        &encode_maintenance_snapshot(&snapshot).unwrap(),
        records.len(),
    )
    .waste
}

fn decode_section(payload: &[u8], count: usize) -> MaintenanceSnapshot {
    let profile = AssetId([7; 16]);
    let mut bytes = Vec::new();
    initialize_empty_world_snapshot_container(&mut bytes, profile);
    for kind in WorldSnapshotSectionKind::ENCODING_ORDER {
        append_encoded_world_snapshot_section_to_container(
            &mut bytes,
            kind,
            if kind == WorldSnapshotSectionKind::Maintenance {
                count as u32
            } else {
                0
            },
            |bytes| {
                if kind == WorldSnapshotSectionKind::Maintenance {
                    bytes.extend_from_slice(payload);
                }
            },
        )
        .unwrap();
    }
    finalize_world_snapshot_container_header_and_checksum(&mut bytes, count as u32).unwrap();
    let validated =
        validate_world_snapshot_container_and_section_directory(&bytes, profile).unwrap();
    assert_eq!(validated.container_header.format_version, 13);
    decode_maintenance_snapshot(
        &bytes,
        validated.section_directory_entry(WorldSnapshotSectionKind::Maintenance),
    )
    .unwrap()
}

#[test]
fn object_condition_and_bin_contents_round_trip_beside_legacy_waste_arrays() {
    let waste = LooseWasteSnapshotRecord {
        kind: Default::default(),
        persistent_identifier: 5,
        definition: AssetId([3; 16]),
        translation: [1.0, 2.0, 3.0],
        rotation: [0.0, 0.0, 0.0, 1.0],
        scale: [1.0; 3],
        units: 2,
    };
    let snapshot = MaintenanceSnapshot {
        waste: vec![waste.clone()],
        objects: vec![
            MaintainedObjectSnapshotRecord {
                persistent_identifier: 7,
                condition_permille: Some(640),
                contained_waste: None,
                next_waste_production_tick: None,
                compost_poo_level: None,
            },
            MaintainedObjectSnapshotRecord {
                persistent_identifier: 9,
                condition_permille: Some(1_000),
                contained_waste: Some([3, 10]),
                next_waste_production_tick: None,
                compost_poo_level: Some(35.0),
            },
        ],
    };
    assert_eq!(
        decode_section(&encode_maintenance_snapshot(&snapshot).unwrap(), 3),
        snapshot
    );
    // Saves written before object state hold a bare waste array.
    let mut legacy = serde_json::to_value(&vec![waste.clone()]).unwrap();
    legacy[0].as_object_mut().unwrap().remove("kind");
    let legacy = serde_json::to_vec(&legacy).unwrap();
    assert_eq!(
        decode_section(&legacy, 1),
        MaintenanceSnapshot {
            waste: vec![waste],
            objects: Vec::new(),
        }
    );
}

#[test]
fn litter_and_animal_waste_round_trip_with_their_distinct_cleanup_kinds() {
    use openzt2_zoo_state::maintenance::maintenance_types::LooseLitterWaste;

    let (mut source, source_root) = fixture();
    let world = WorldMember { root: source_root };
    source.world_mut().spawn((
        PersistentId(11),
        DefinitionId(AssetId([3; 16])),
        Transform::from_translation(Vec3::new(2.0, 0.0, 3.0)),
        world,
        AnimalHabitatWaste {
            uncontained_waste_units: 2,
        },
    ));
    source.world_mut().spawn((
        PersistentId(13),
        DefinitionId(AssetId([5; 16])),
        Transform::from_translation(Vec3::new(4.0, 0.0, 5.0)),
        world,
        LooseLitterWaste {
            uncontained_waste_units: 3,
        },
    ));
    let records = round_trip(&capture(source.world_mut()));
    let (mut destination, destination_root) = fixture();
    restore(&mut destination, destination_root, &records, &[None, None]);
    assert_eq!(capture(destination.world_mut()), records);
}

#[test]
fn maintained_object_state_compost_and_animal_waste_deadlines_survive_entity_remapping() {
    use super::maintenance_snapshot_application::apply_maintained_object_records;
    use super::maintenance_snapshot_capture::{
        capture_maintained_object_records, MaintainedObjectSnapshotEntities,
    };
    use openzt2_zoo_state::maintenance::maintenance_types::{
        AnimalWasteProductionSchedule, CompostPooLevel, FacilityContainedWaste,
        MaintainableObjectConditionPermille,
    };

    let (mut source, _) = fixture();
    source.world_mut().spawn((
        PersistentId(7),
        MaintainableObjectConditionPermille(640),
        FacilityContainedWaste {
            contained_waste_units: 3,
            contained_waste_capacity_units: 10,
        },
    ));
    source.world_mut().spawn((
        PersistentId(9),
        AnimalWasteProductionSchedule {
            next_production_tick: 293,
        },
    ));
    source
        .world_mut()
        .spawn((PersistentId(11), CompostPooLevel(35.0)));
    let mut capture = SystemState::<MaintainedObjectSnapshotEntities>::new(source.world_mut());
    let objects = capture_maintained_object_records(&capture.get(source.world()).unwrap());
    let snapshot = MaintenanceSnapshot {
        waste: Vec::new(),
        objects,
    };
    let decoded = decode_section(
        &encode_maintenance_snapshot(&snapshot).unwrap(),
        snapshot.objects.len(),
    );

    let mut destination = App::new();
    destination.add_plugins(openzt2_zoo_staff::maintenance::MaintenancePlugin);
    for _ in 0..8 {
        destination.world_mut().spawn_empty();
    }
    let facility = destination
        .world_mut()
        .spawn((
            PersistentId(7),
            MaintainableObjectConditionPermille(1_000),
            FacilityContainedWaste {
                contained_waste_units: 0,
                contained_waste_capacity_units: 10,
            },
        ))
        .id();
    let animal = destination.world_mut().spawn(PersistentId(9)).id();
    let compost = destination.world_mut().spawn(PersistentId(11)).id();
    let identities = std::collections::BTreeMap::from([(7, facility), (9, animal), (11, compost)]);
    let mut commands = SystemState::<Commands>::new(destination.world_mut());
    apply_maintained_object_records(
        &mut commands.get_mut(destination.world_mut()).unwrap(),
        &decoded.objects,
        &identities,
    );
    commands.apply(destination.world_mut());
    let mut capture = SystemState::<MaintainedObjectSnapshotEntities>::new(destination.world_mut());
    assert_eq!(
        capture_maintained_object_records(&capture.get(destination.world()).unwrap()),
        snapshot.objects
    );
    let totals = destination
        .world()
        .resource::<IncrementalZooCleanlinessTotals>();
    assert_eq!(totals.maintainable_object_count, 1);
    assert_eq!(totals.maintainable_condition_permille_sum, 640);
    assert_eq!(totals.contained_waste_units, 3);
    assert_eq!(totals.contained_waste_capacity_units, 10);
}

#[test]
fn maintenance_encoder_rejects_a_combined_record_count_the_decoder_cannot_read() {
    let snapshot = MaintenanceSnapshot {
        waste: vec![LooseWasteSnapshotRecord {
            kind: Default::default(),
            persistent_identifier: 1,
            definition: AssetId([3; 16]),
            translation: [0.0; 3],
            rotation: [0.0, 0.0, 0.0, 1.0],
            scale: [1.0; 3],
            units: 1,
        }],
        objects: (0..super::maintenance_snapshot_section_encoding::MAXIMUM_WASTE_RECORDS)
            .map(|index| MaintainedObjectSnapshotRecord {
                persistent_identifier: index as u64 + 2,
                condition_permille: Some(1_000),
                contained_waste: None,
                next_waste_production_tick: None,
                compost_poo_level: None,
            })
            .collect(),
    };
    assert_eq!(
        encode_maintenance_snapshot(&snapshot),
        Err(super::super::persistence_failure_types::WorldSnapshotPersistenceFailure::CapacityExceeded)
    );
}

#[test]
fn restored_waste_rebuilds_one_job_and_authored_cleanup_survives_a_second_save() {
    let (mut source, source_root) = fixture();
    source.world_mut().spawn((
        AnimalHabitatWaste {
            uncontained_waste_units: 7,
        },
        PersistentId(51),
        DefinitionId(AssetId::from_key("test_waste")),
        WorldMember { root: source_root },
        Transform::from_xyz(8.25, 1.5, -4.0)
            .with_rotation(Quat::from_rotation_y(0.6))
            .with_scale(Vec3::splat(1.25)),
    ));
    source.update();
    let expected = capture(source.world_mut());
    let records = round_trip(&expected);
    let (mut destination, root) = fixture();
    for _ in 0..8 {
        destination.world_mut().spawn_empty();
    }
    let baseline = destination
        .world_mut()
        .spawn((
            AnimalHabitatWaste {
                uncontained_waste_units: 2,
            },
            PersistentId(51),
            WorldMember { root },
        ))
        .id();
    restore(&mut destination, root, &records, &[None]);
    assert!(destination.world().get_entity(baseline).is_err());
    assert_eq!(capture(destination.world_mut()), expected);
    assert_eq!(
        destination
            .world()
            .resource::<IncrementalZooCleanlinessTotals>()
            .uncontained_waste_units,
        7
    );
    assert_eq!(
        destination
            .world()
            .resource::<PersistentIdAllocator>()
            .next(),
        52
    );
    destination.update();
    destination.update();
    let jobs: Vec<_> = destination
        .world_mut()
        .query::<(Entity, &StaffJob)>()
        .iter(destination.world())
        .map(|(entity, job)| (entity, *job))
        .collect();
    assert_eq!(jobs.len(), 1);
    let (job, work) = jobs[0];
    assert_eq!(work.kind, StaffJobKind::CleanHabitat);
    assert_eq!(work.token, Some(AssetId::from_key("t_rakepoo")));
    assert_eq!(
        destination.world().get::<PersistentId>(work.target),
        Some(&PersistentId(51))
    );
    assert_eq!(
        destination
            .world()
            .get::<WorldMember>(work.target)
            .unwrap()
            .root,
        root
    );
    let staff = destination.world_mut().spawn(Staff).id();
    let document = destination
        .world_mut()
        .resource_mut::<Assets<BehaviorDocumentAsset>>()
        .add(BehaviorDocumentAsset::from_test_source(
            br#"<BFAITaskTemplateList>
        <BFAITaskTemplate Name="Clean" UniqueID="worker:rakepoo">
        <BFBehExecTask><BFBehKill killTarget="true"/></BFBehExecTask>
        </BFAITaskTemplate></BFAITaskTemplateList>"#,
        ));
    destination.world_mut().entity_mut(job).insert((
        JobClaim { staff },
        StaffJobBehaviorTask {
            document: document.clone(),
            task: AssetId::from_key("worker:rakepoo"),
            declaration_index: 0,
            execution_id: Some(99),
        },
    ));
    destination.world_mut().entity_mut(staff).insert((
        CurrentJob { job },
        BehaviorTaskExecutionState {
            execution_id: 99,
            program: AssetId::from_key("worker:rakepoo"),
            target: Some(work.target),
            document,
            declaration: 0,
            phase: BehaviorTaskExecutionPhase::Execution,
            action: 0,
            repetitions: 0,
            next_action_tick: 0,
            object: None,
            interaction_slot: None,
            stack: Default::default(),
        },
    ));
    destination.update();
    assert!(destination.world().get_entity(work.target).is_err());
    assert_eq!(
        destination
            .world()
            .resource::<IncrementalZooCleanlinessTotals>()
            .uncontained_waste_units,
        0
    );
    // Save after the kill but before the task's terminal event: consumed waste stays gone.
    let cleaned = round_trip(&capture(destination.world_mut()));
    assert!(cleaned.is_empty());
    destination.world_mut().write_message(BehaviorTaskFinished {
        actor: staff,
        execution_id: 99,
    });
    destination.world_mut().write_message(BehaviorTaskFinished {
        actor: staff,
        execution_id: 99,
    });
    destination.update();
    assert_eq!(
        destination
            .world_mut()
            .resource_mut::<Messages<StaffJobCompleted>>()
            .drain()
            .count(),
        1
    );
    let (mut reloaded, root) = fixture();
    let baseline = reloaded
        .world_mut()
        .spawn((
            AnimalHabitatWaste {
                uncontained_waste_units: 5,
            },
            WorldMember { root },
        ))
        .id();
    restore(&mut reloaded, root, &cleaned, &[]);
    reloaded.update();
    assert!(reloaded.world().get_entity(baseline).is_err());
    assert_eq!(
        reloaded
            .world_mut()
            .query::<&StaffJob>()
            .iter(reloaded.world())
            .count(),
        0
    );
    assert_eq!(
        reloaded
            .world()
            .resource::<IncrementalZooCleanlinessTotals>()
            .uncontained_waste_units,
        0
    );
}

#[test]
fn restored_prefab_preserves_saved_root_pose_and_removes_its_children_with_waste() {
    use openzt2_game_data::scene_prefab::{
        PrefabEntity, PrefabTransform, ScenePrefabDocument, ScenePrefabEntityFlags,
    };
    let (mut app, root) = fixture();
    let entity = |children| PrefabEntity {
        stable_id: AssetId([3; 16]),
        attachment_id: AssetId([4; 16]),
        model_joint_binding: None,
        local_render_bound: None,
        skin_render_bound: None,
        parent: 0,
        transform: PrefabTransform {
            translation_m: [3.0; 3],
            rotation_xyzw: [0.0, 0.0, 0.0, 1.0],
            scale: [2.0; 3],
        },
        children,
        flags: ScenePrefabEntityFlags(
            ScenePrefabEntityFlags::VISIBLE.0 | ScenePrefabEntityFlags::ACTIVE.0,
        ),
        rotation_cycles: vec![],
        transform_animations: vec![],
        renderables: vec![],
        colliders: vec![],
        lods: vec![],
        billboards: vec![],
        lights: vec![],
        effects: vec![],
    };
    let prefab = app
        .world_mut()
        .resource_mut::<Assets<ScenePrefabAsset>>()
        .add(ScenePrefabAsset::from_test_document(ScenePrefabDocument {
            entities: vec![entity(vec![1]), entity(vec![])],
            automatic_placement_bounds_xz: None,
            rail_camera: None,
            dependencies: vec![],
        }));
    let record = LooseWasteSnapshotRecord {
        kind: Default::default(),
        persistent_identifier: 7,
        definition: AssetId::from_key("test_waste"),
        translation: [9.0, 2.0, 8.0],
        rotation: Quat::from_rotation_y(0.3).to_array(),
        scale: [1.25; 3],
        units: 3,
    };
    restore(
        &mut app,
        root,
        std::slice::from_ref(&record),
        &[Some(prefab)],
    );
    assert_eq!(capture(app.world_mut()), vec![record]);
    let waste = app
        .world_mut()
        .query_filtered::<Entity, With<AnimalHabitatWaste>>()
        .single(app.world())
        .unwrap();
    let child = app.world().get::<Children>(waste).unwrap()[0];
    assert_eq!(
        app.world().get::<Transform>(child).unwrap().translation,
        Vec3::splat(3.0)
    );
    assert!(app.world_mut().despawn(waste));
    assert!(app.world().get_entity(child).is_err());
    assert_eq!(
        app.world()
            .resource::<IncrementalZooCleanlinessTotals>()
            .uncontained_waste_units,
        0
    );
}
