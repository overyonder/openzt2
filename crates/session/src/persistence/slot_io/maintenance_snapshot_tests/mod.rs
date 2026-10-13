use super::{
    super::{
        snapshot_container_encoding_and_validation::*,
        snapshot_container_types::WorldSnapshotSectionKind,
    },
    maintenance_snapshot_application::apply_habitat_waste_records,
    maintenance_snapshot_capture::{capture_habitat_waste_records, HabitatWasteSnapshotEntities},
    maintenance_snapshot_section_encoding::{
        decode_habitat_waste_records, encode_habitat_waste_records,
    },
    maintenance_snapshot_types::HabitatWasteSnapshotRecord,
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
    add_animal_waste_cleanup_test_systems(&mut app);
    let root = app.world_mut().spawn_empty().id();
    app.insert_resource(PersistentIdAllocator::new(root));
    (app, root)
}

fn capture(world: &mut World) -> Vec<HabitatWasteSnapshotRecord> {
    let mut state = SystemState::<HabitatWasteSnapshotEntities>::new(world);
    capture_habitat_waste_records(&state.get(world).unwrap()).unwrap()
}

fn restore(
    app: &mut App,
    root: Entity,
    records: &[HabitatWasteSnapshotRecord],
    prefabs: &[Option<Handle<ScenePrefabAsset>>],
) {
    let mut state = SystemState::<(
        Commands,
        ResMut<PersistentIdAllocator>,
        Res<Assets<ScenePrefabAsset>>,
        Query<Entity, With<AnimalHabitatWaste>>,
    )>::new(app.world_mut());
    {
        let (mut commands, mut ids, assets, baseline) = state.get_mut(app.world_mut()).unwrap();
        apply_habitat_waste_records(
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

fn round_trip(records: &[HabitatWasteSnapshotRecord]) -> Vec<HabitatWasteSnapshotRecord> {
    let profile = AssetId([7; 16]);
    let payload = encode_habitat_waste_records(records).unwrap();
    let mut bytes = Vec::new();
    initialize_empty_world_snapshot_container(&mut bytes, profile);
    for kind in WorldSnapshotSectionKind::ENCODING_ORDER {
        append_encoded_world_snapshot_section_to_container(
            &mut bytes,
            kind,
            if kind == WorldSnapshotSectionKind::Maintenance {
                records.len() as u32
            } else {
                0
            },
            |bytes| {
                if kind == WorldSnapshotSectionKind::Maintenance {
                    bytes.extend_from_slice(&payload);
                }
            },
        )
        .unwrap();
    }
    finalize_world_snapshot_container_header_and_checksum(&mut bytes, records.len() as u32)
        .unwrap();
    let validated =
        validate_world_snapshot_container_and_section_directory(&bytes, profile).unwrap();
    assert_eq!(validated.container_header.format_version, 13);
    decode_habitat_waste_records(
        &bytes,
        validated.section_directory_entry(WorldSnapshotSectionKind::Maintenance),
    )
    .unwrap()
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
    let record = HabitatWasteSnapshotRecord {
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
