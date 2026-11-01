use super::*;
use arrayvec::ArrayVec;
use bevy::asset::{
    io::{
        memory::{Dir, MemoryAssetReader},
        AssetSourceBuilder, AssetSourceId, Reader,
    },
    AssetLoader, LoadContext,
};
use openzt2_game_data::{
    behavior::{
        action::spawn::BehaviorSpawnPosePolicy,
        document::{BehaviorDocument, BehaviorSet, BehaviorTaskActionPhase},
    },
    scene_prefab::{PrefabEntity, PrefabTransform, ScenePrefabDocument, ScenePrefabEntityFlags},
    world_definitions::{
        document::WorldDefinitionDocument,
        simulation_time::{
            SimulationCalendarLeapPolicy, SimulationCalendarPolicy, SimulationTimingDefinition,
        },
        world_objects::*,
    },
};
use openzt2_world_state::simulation_time::deterministic_random_stream::{
    DeterministicRng, RngDomain, ZooSeed,
};
use openzt2_world_state::world_spawn::persistent_id_types::PersistentId;
use openzt2_zoo_state::behavior_task_execution_types::BehaviorTaskExecutionPhase;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// Loads any `.testprefab` as a one-entity scene once the test opens the gate.
#[derive(TypePath)]
struct GatedPrefabLoader(Arc<AtomicBool>);

impl AssetLoader for GatedPrefabLoader {
    type Asset = ScenePrefabAsset;
    type Settings = ();
    type Error = std::io::Error;

    async fn load(
        &self,
        _reader: &mut dyn Reader,
        _settings: &(),
        _load_context: &mut LoadContext<'_>,
    ) -> Result<ScenePrefabAsset, std::io::Error> {
        while !self.0.load(Ordering::Acquire) {
            bevy::tasks::futures_lite::future::yield_now().await;
        }
        Ok(ScenePrefabAsset::from_test_document(ScenePrefabDocument {
            entities: vec![PrefabEntity {
                stable_id: AssetId::from_key("root"),
                attachment_id: AssetId::from_key("root"),
                model_joint_binding: None,
                local_render_bound: None,
                skin_render_bound: None,
                parent: 0,
                transform: PrefabTransform {
                    translation_m: [0.0; 3],
                    rotation_xyzw: [0.0, 0.0, 0.0, 1.0],
                    scale: [1.0; 3],
                },
                children: vec![],
                flags: ScenePrefabEntityFlags(ScenePrefabEntityFlags::ACTIVE.0),
                rotation_cycles: vec![],
                transform_animations: vec![],
                renderables: vec![],
                colliders: vec![],
                lods: vec![],
                billboards: vec![],
                lights: vec![],
                effects: vec![],
            }],
            automatic_placement_bounds_xz: None,
            rail_camera: None,
            dependencies: vec![],
        }))
    }

    fn extensions(&self) -> &[&str] {
        &["testprefab"]
    }
}

fn litter(id: AssetId) -> WorldObjectDefinition {
    WorldObjectDefinition {
        audio_cue_prefixes: Vec::new(),
        surface_effect_strength: None,
        presentation_caustic_policies: Vec::new(),
        id,
        kind: WorldObjectKind::Scenery,
        water_filter: None,
        supports_show_tricks: false,
        behavior_type_identifiers: Vec::new(),
        view_data: Vec::new(),
        detach_actions: vec![],
        selected_ui_broadcasts: vec![],
        information_panel: AssetId::default(),
        view_class: None,
        biome_automatic_placement_class: None,
        name_key: id,
        description_key: id,
        zoopedia_subject: id,
        prefab: id,
        catalogue_preview_prefab: None,
        presentation_attachments: vec![],
        named_physical_presentations: vec![],
        interaction_slots: vec![],
        container_quantity: None,
        transactions: vec![],
        prefab_scale: 1.0,
        icon: id,
        biomes: vec![],
        location: id,
        preview_offset_cm: [0; 3],
        preview_scale: 1.0,
        terrain_fitted: false,
        real_physics_water_impact: None,
        price_cents: 0,
        upkeep_cents_per_month: 0,
        properties: WorldObjectPropertyFlags::LOOSE_LITTER,
        affordances: Default::default(),
        destruction: None,
    }
}

#[test]
fn dropped_litter_waits_for_its_model_and_spawns_once() {
    let dir = Dir::default();
    dir.insert_asset(Path::new("trash.testprefab"), Vec::<u8>::new());
    let gate = Arc::new(AtomicBool::new(false));
    let mut app = App::new();
    app.register_asset_source(
        AssetSourceId::Default,
        AssetSourceBuilder::new(move || Box::new(MemoryAssetReader { root: dir.clone() })),
    )
    .add_plugins((
        TaskPoolPlugin::default(),
        AssetPlugin {
            watch_for_changes_override: Some(false),
            use_asset_processor_override: Some(false),
            ..default()
        },
    ))
    .init_asset::<ScenePrefabAsset>()
    .register_asset_loader(GatedPrefabLoader(gate.clone()))
    .init_asset::<BehaviorDocumentAsset>()
    .init_asset::<WorldDefinitionAsset>()
    .init_asset::<openzt2_game_assets::terrain::terrain_asset_types_and_borrowing_queries::TerrainAsset>()
    .init_resource::<WorldDefinitions>()
    .init_resource::<openzt2_zoo_state::topology::topology_graph_types::TopologyGrid>()
    .init_resource::<openzt2_zoo_state::habitat::habitat_types::HabitatIndex>()
    .init_resource::<openzt2_zoo_state::placement::placement_occupancy_index::PlacedObjectFootprintOccupancyIndex>()
    .init_resource::<openzt2_zoo_state::terrain::terrain_chunk_types::TerrainIndex>()
    .insert_resource(ZooClock {
        tick: 1,
        absolute_day: 0,
        tick_in_day: 1,
        fractional_tick: Default::default(),
    })
    .add_systems(Update, spawn_behavior_objects_at_actor);

    let trash = AssetId::from_key("TrashGround");
    let definitions = WorldDefinitionDocument {
        objects: vec![litter(trash)],
        simulation_timing: Some(SimulationTimingDefinition {
            fixed_hz: 10,
            ticks_per_day: 10,
            speed_multipliers: vec![],
            calendar: SimulationCalendarPolicy {
                epoch_year: 2000,
                epoch_month: 1,
                epoch_day: 1,
                month_lengths: [30; 12],
                leap: SimulationCalendarLeapPolicy::None,
            },
        }),
        ..default()
    };
    let handle = app
        .world_mut()
        .resource_mut::<Assets<WorldDefinitionAsset>>()
        .add(
            WorldDefinitionAsset::from_test_document(definitions.clone())
                .with_test_scene(trash, "trash.testprefab"),
        );
    let server = app.world().resource::<AssetServer>().clone();
    app.world_mut()
        .resource_scope(|world, mut world_definitions: Mut<WorldDefinitions>| {
            let assets = world.resource::<Assets<WorldDefinitionAsset>>();
            world_definitions.index_test_document(handle.clone(), &definitions);
            world_definitions.index_test_scenes(server, &handle, assets.get(&handle).unwrap());
        });

    let program = AssetId::from_key("ThrowAwayTrashGround");
    let document = app
        .world_mut()
        .resource_mut::<Assets<BehaviorDocumentAsset>>()
        .add(BehaviorDocumentAsset::from_test_document(
            BehaviorDocument::Sets(vec![BehaviorSet {
                id: program,
                name: "ThrowAwayTrashGround".into(),
                subjects: vec![],
                actions: BehaviorTaskActionPhase::Supported(vec![
                    BehaviorAction::SpawnObjectAtActor {
                        entity: trash,
                        pose: BehaviorSpawnPosePolicy::default(),
                    },
                ]),
            }]),
        ));
    let root = app.world_mut().spawn_empty().id();
    app.insert_resource(PersistentIdAllocator::new(root));
    let guest = app
        .world_mut()
        .spawn((
            WorldMember { root },
            Transform::default(),
            GlobalTransform::default(),
            GuestRng(DeterministicRng::from_entity(
                ZooSeed(1),
                PersistentId(1),
                RngDomain::Guest,
            )),
            BehaviorTaskExecutionState {
                execution_id: 1,
                program,
                target: None,
                object: None,
                document,
                declaration: 0,
                phase: BehaviorTaskExecutionPhase::Set,
                action: 0,
                repetitions: 0,
                next_action_tick: 0,
                interaction_slot: None,
                stack: ArrayVec::new(),
            },
        ))
        .id();
    let litter_count = |app: &mut App| {
        app.world_mut()
            .query::<&LooseLitterWaste>()
            .iter(app.world())
            .count()
    };

    // The model stays unloaded behind the gate for many frames.
    for _ in 0..20 {
        app.update();
    }
    assert_eq!(litter_count(&mut app), 0);
    assert_eq!(
        app.world()
            .get::<BehaviorTaskExecutionState>(guest)
            .unwrap()
            .action,
        0
    );
    assert!(app
        .world()
        .get::<PendingAtActorSpawnPrefab>(guest)
        .is_some());

    gate.store(true, Ordering::Release);
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while litter_count(&mut app) == 0 && std::time::Instant::now() < deadline {
        app.update();
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
    for _ in 0..5 {
        app.update();
    }
    assert_eq!(litter_count(&mut app), 1);
    assert_eq!(
        app.world()
            .get::<BehaviorTaskExecutionState>(guest)
            .unwrap()
            .action,
        1
    );
    assert!(app
        .world()
        .get::<PendingAtActorSpawnPrefab>(guest)
        .is_none());
}
