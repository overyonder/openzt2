use bevy::prelude::*;
use openzt2_game_assets::{
    application_lifecycle::GamePhase,
    scene_prefab::ScenePrefabAsset,
    species::species_asset_types::{SpeciesAsset, SpeciesAssets},
    terrain::{
        terrain_asset_types_and_borrowing_queries::TerrainAsset,
        terrain_test_asset::terrain_test_asset,
    },
    world_definitions::{
        world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions,
        world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset,
    },
};
use openzt2_game_data::{
    species::*,
    terrain::*,
    world_definitions::{document::WorldDefinitionDocument, simulation_time::*, world_objects::*},
    AssetId,
};
use openzt2_world_state::{
    application_schedule::FixedGameSet,
    simulation_time::{
        deterministic_random_stream::ZooSeed,
        simulation_clock_types::{ZooCalendar, ZooClock, ZooDay, ZooDayAdvanced, ZooMonth},
        simulation_control_queries::simulation_is_running,
        simulation_control_types::{SimulationControl, SimulationPaused, SimulationSpeedTier},
    },
    world_spawn::{
        persistent_id_types::PersistentIdAllocator,
        world_membership_types::{WorldMember, WorldRoot},
    },
};
use openzt2_zoo_state::{
    animal_health::types::AnimalDied,
    animal_lifecycle::{
        animal_adoption_contracts::{
            AdoptAnimal, AnimalAdopted, AnimalAdoptionRefund, PendingAdoption,
        },
        types::Animal,
    },
    construction::FixedConstructionSet,
    economy::{
        account_transaction_types::{TransactionCompleted, TransactionKind, TransactionRejected},
        money_types::Money,
        zoo_cash_types::ZooCash,
    },
    guests::guest_simulation_types::{GuestArrived, GuestDeparted, GuestReachedEntrance},
    habitat::habitat_types::HabitatIndex,
    terrain::terrain_chunk_types::{TerrainChunk, TerrainIndex},
    topology::topology_graph_types::TopologyGrid,
};

fn species_document() -> SpeciesDocument {
    let id = AssetId::from_key("test_species");
    SpeciesDocument {
        species: vec![Species {
            caffeine_factors: Default::default(),
            placement_surface_policy: Default::default(),
            donation: None,
            id,
            authored_species_name: "test_species".to_owned(),
            authored_product_name: String::new(),
            name_key: id,
            taxonomy_key: id,
            world_definition: id,
            needs: vec![],
            habitat_space: None,
            adult_mass_kg: [1.0; 2],
            stage_start_days: [0.0, 0.0, 10.0, 100.0],
            lifespan_days: [100.0; 2],
            gestation_days: 5.0,
            litter: [2, 4],
            move_speed_mps: 1.0,
            swim_speed_mps: 0.0,
            appetite_per_day: 0.0,
            waste_definition: id,
            waste_interval_days: 1.0,
            waste_units: 1,
            keeper_food_definition: None,
            keeper_drink_definition: None,
            conservation: ConservationStatus::Endangered,
            flags: SpeciesFlags::ADOPTABLE,
            adoption_offer: AnimalAdoptionOfferDefinition {
                rarity_fame_percent: 0,
                unlock_seconds_range: [0.0; 2],
                remove_after_seconds: 0.0,
                dismiss_cooldown_seconds: 0.0,
            },
        }],
        variants: [Sex::Female, Sex::Male]
            .into_iter()
            .enumerate()
            .map(|(index, sex)| SpeciesVariantBinding {
                species: id,
                authored_species_name_override: None,
                authored_product_name_override: None,
                variant: SpeciesVariant {
                    need_points: Default::default(),
                    placement_surface_policy: Default::default(),
                    id: AssetId::from_key(&format!("young_{index}")),
                    behavior_subject_type_identifiers: vec![],
                    navigation_collision_policy: None,
                    virtual_nodes: Vec::new(),
                    ground_navigation_clip_asset_key: None,
                    medium_ground_navigation_clip_asset_key: None,
                    fast_ground_navigation_clip_asset_key: None,
                    named_ground_navigation_clip_asset_keys: Vec::new(),
                    surface_water_locomotion: None,
                    sex,
                    life_stage: LifeStage::Adult,
                    model: format!("young_{index}.nif"),
                    model_animation_set: id,
                    initial_animation_clip_asset_key: None,
                    initial_animation_loops: false,
                    material_variant: None,
                    scale: [0.5, 0.75],
                    weight_kg: [1.0; 2],
                    probability: 1,
                    flags: SpeciesVariantFlags::default(),
                    adoption_count_range: [1; 2],
                },
            })
            .collect(),
        compatibilities: vec![],
    }
}

fn object(id: AssetId) -> WorldObjectDefinition {
    WorldObjectDefinition {
        collision_scores: None,
        authored_runtime: Default::default(),
        audio_cue_prefixes: Vec::new(),
        audio_stage_volumes: Vec::new(),
        surface_effect_strength: None,
        presentation_caustic_policies: Vec::new(),
        traversal_animation_manifests: Vec::new(),
        id,
        kind: WorldObjectKind::Animal,
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
        terrain_fit_height_offset_m: 0.0,
        terrain_fit_water_offset_m: None,
        real_physics_water_impact: None,
        price_cents: 1_250,
        upkeep_cents_per_month: 0,
        properties: Default::default(),
        affordances: Default::default(),
        destruction: None,
        virtual_nodes: Vec::new(),
    }
}

fn setup(cash: i64) -> (App, Entity) {
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, AssetPlugin::default()))
        .insert_resource(State::new(GamePhase::InGame))
        .add_message::<GuestArrived>()
        .add_message::<GuestDeparted>()
        .add_message::<GuestReachedEntrance>()
        .add_message::<ZooDayAdvanced>()
        .add_message::<AnimalDied>()
        .init_asset::<ScenePrefabAsset>()
        .init_resource::<Assets<SpeciesAsset>>()
        .init_resource::<SpeciesAssets>()
        .init_resource::<Assets<WorldDefinitionAsset>>()
        .init_resource::<WorldDefinitions>()
        .init_resource::<Assets<TerrainAsset>>()
        .add_plugins((
            super::AnimalLifecyclePlugin,
            openzt2_economy::economy::EconomyPlugin,
        ));
    app.configure_sets(
        FixedUpdate,
        (
            FixedGameSet::Clock,
            FixedGameSet::Think,
            FixedGameSet::Navigate,
            FixedGameSet::Act,
            FixedGameSet::Economy,
            FixedGameSet::Cleanup,
        )
            .chain()
            .run_if(simulation_is_running),
    );
    app.configure_sets(
        FixedUpdate,
        (
            FixedConstructionSet::Prepare,
            FixedConstructionSet::RequestEconomy,
            FixedConstructionSet::SettleEconomy,
            FixedConstructionSet::CompleteEconomy,
            FixedConstructionSet::Apply,
            FixedConstructionSet::Cleanup,
        )
            .chain()
            .after(FixedGameSet::Cleanup),
    );
    let document = species_document();
    let species = document.species[0].id;
    let handle = app
        .world_mut()
        .resource_mut::<Assets<SpeciesAsset>>()
        .add(SpeciesAsset::from_test_document(document.clone()));
    app.world_mut()
        .resource_mut::<SpeciesAssets>()
        .index_test_document(handle, &document);
    let definitions = WorldDefinitionDocument {
        objects: vec![object(species)],
        simulation_timing: Some(SimulationTimingDefinition {
            fixed_hz: 10,
            ticks_per_day: 100,
            speed_multipliers: vec![],
            calendar: SimulationCalendarPolicy {
                epoch_year: 2000,
                epoch_month: 1,
                epoch_day: 1,
                month_lengths: [30; 12],
                leap: SimulationCalendarLeapPolicy::None,
            },
        }),
        ..Default::default()
    };
    let handle = app
        .world_mut()
        .resource_mut::<Assets<WorldDefinitionAsset>>()
        .add(WorldDefinitionAsset::from_test_document(
            definitions.clone(),
        ));
    app.world_mut()
        .resource_mut::<WorldDefinitions>()
        .index_test_document(handle, &definitions);
    let root = app
        .world_mut()
        .spawn((
            WorldRoot { scenario: species },
            PersistentIdAllocator::default(),
            SimulationControl::new(true),
            SimulationPaused(true),
            SimulationSpeedTier::default(),
            ZooSeed(123),
            ZooClock {
                tick: 99,
                absolute_day: 0,
                tick_in_day: 99,
                fractional_tick: Default::default(),
            },
            ZooCalendar {
                year: 2000,
                month: 1,
                day: 1,
            },
            ZooDay(0),
            ZooMonth(0),
            TopologyGrid::default(),
            HabitatIndex::default(),
            ZooCash(Money::from_cents(cash)),
        ))
        .id();
    let grid = TerrainGrid {
        width: 2,
        height: 2,
        sector_columns: 1,
        sector_rows: 1,
        units: TerrainUnits {
            cell_size_metres: 1.0,
            base_height_metres: 0.0,
            maximum_height_metres: 0.0,
            minimum_water_height_metres: None,
        },
        presentation: TerrainPresentation {
            detail_texture: String::new(),
            detail_texture_repetition: 1.0,
            cliff_rise_per_metre: [0.0; 2],
            surface_resolution: 1,
            surface_margin_cells: 0,
            full_alpha_composition: false,
            water_distance_fade: Default::default(),
            water_editing: Default::default(),
        },
        biomes: vec![],
        samples: vec![
            TerrainSample {
                height_relative_to_base_metres: 0.0,
                biome_index: 0,
                has_ground_cover: false,
                water_depth: TerrainWaterDepth::Dry,
                water_surface_flag: false,
                excludes_elevated_path: false,
                support_floor_metres: None,
                height_cap_metres: None,
                height_is_linked: false,
                has_child_height_link: false,
            };
            4
        ],
        water_regions: vec![],
        waterfall: None,
        slope_tiles: vec![TerrainSlopeTile {
            uses_alternate_diagonal: false,
        }],
    };
    let asset = app
        .world_mut()
        .resource_mut::<Assets<TerrainAsset>>()
        .add(terrain_test_asset(grid));
    let chunk = app
        .world_mut()
        .spawn((
            WorldMember { root },
            TerrainChunk {
                source_coord: IVec2::ZERO,
                coord: IVec2::ZERO,
                asset,
                origin: Vec2::ZERO,
                spacing_m: 1.0,
                side: 2,
            },
        ))
        .id();
    let mut index = TerrainIndex {
        chunk_span_m: 1.0,
        ..Default::default()
    };
    index.chunks.insert(IVec2::ZERO, chunk);
    app.world_mut().entity_mut(root).insert(index);
    app.world_mut().write_message(AdoptAnimal {
        species,
        variant: None,
        sex: Some(Sex::Female),
        offer_slot_index: None,
        world: root,
        habitat: None,
        transform: Transform::from_xyz(0.5, 0.0, 0.5),
    });
    (app, root)
}

fn advance_fixed(app: &mut App) {
    app.world_mut().run_schedule(FixedUpdate);
}

fn animal_count(app: &mut App) -> usize {
    let world = app.world_mut();
    world
        .query_filtered::<Entity, With<Animal>>()
        .iter(world)
        .count()
}

fn assert_no_pending_adoption(app: &mut App) {
    let world = app.world_mut();
    assert_eq!(
        world
            .query_filtered::<Entity, With<PendingAdoption>>()
            .iter(world)
            .count(),
        0
    );
    assert_eq!(
        world
            .query_filtered::<Entity, With<AnimalAdoptionRefund>>()
            .iter(world)
            .count(),
        0
    );
}

#[test]
fn paused_adoption_charges_and_spawns_once_without_an_enclosure() {
    let (mut app, root) = setup(10_000);
    advance_fixed(&mut app);
    assert_eq!(animal_count(&mut app), 1);
    assert_eq!(
        app.world().get::<ZooCash>(root).unwrap().0,
        Money::from_cents(8_750)
    );
    assert_eq!(app.world().resource::<Messages<AnimalAdopted>>().len(), 1);
    for _ in 0..4 {
        advance_fixed(&mut app);
    }
    assert_eq!(animal_count(&mut app), 1);
    assert_eq!(
        app.world().get::<ZooCash>(root).unwrap().0,
        Money::from_cents(8_750)
    );
    assert_eq!(app.world().resource::<Messages<AnimalAdopted>>().len(), 1);
    assert_no_pending_adoption(&mut app);
    assert_eq!(app.world().get::<ZooClock>(root).unwrap().tick, 99);
}

#[test]
fn paused_adoption_with_insufficient_funds_does_not_spawn_or_charge() {
    let (mut app, root) = setup(1_249);
    for _ in 0..4 {
        advance_fixed(&mut app);
    }
    assert_eq!(animal_count(&mut app), 0);
    assert_eq!(
        app.world().get::<ZooCash>(root).unwrap().0,
        Money::from_cents(1_249)
    );
    assert_eq!(
        app.world()
            .resource::<Messages<TransactionRejected>>()
            .len(),
        1
    );
    assert!(app.world().resource::<Messages<AnimalAdopted>>().is_empty());
    assert_no_pending_adoption(&mut app);
}

#[test]
fn failed_paid_spawn_refunds_once_through_a_distinct_live_operation() {
    let (mut app, root) = setup(10_000);
    app.world_mut()
        .entity_mut(root)
        .insert(PersistentIdAllocator::from_snapshot(u64::MAX).unwrap());
    advance_fixed(&mut app);
    assert_eq!(animal_count(&mut app), 0);
    assert_eq!(
        app.world().get::<ZooCash>(root).unwrap().0,
        Money::from_cents(8_750)
    );
    advance_fixed(&mut app);
    assert_eq!(
        app.world().get::<ZooCash>(root).unwrap().0,
        Money::from_cents(10_000)
    );
    for _ in 0..4 {
        advance_fixed(&mut app);
    }
    assert_eq!(
        app.world().get::<ZooCash>(root).unwrap().0,
        Money::from_cents(10_000)
    );
    assert_no_pending_adoption(&mut app);
    let transactions = app.world().resource::<Messages<TransactionCompleted>>();
    let mut cursor = transactions.get_cursor();
    let completed = cursor.read(transactions).collect::<Vec<_>>();
    assert_eq!(completed.len(), 2);
    assert_eq!(completed[0].kind, TransactionKind::AnimalAdoption);
    assert_eq!(completed[1].kind, TransactionKind::Refund);
    assert_ne!(completed[0].operation, completed[1].operation);
    assert!(app.world().resource::<Messages<AnimalAdopted>>().is_empty());
}
