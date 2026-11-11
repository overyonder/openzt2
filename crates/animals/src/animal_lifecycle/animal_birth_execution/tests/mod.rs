use super::*;
use arrayvec::ArrayVec;
use openzt2_game_data::behavior::{
    action::spawn::BehaviorSpawnPosePolicy,
    document::{BehaviorDocument, BehaviorSet, BehaviorTaskActionPhase},
};
use openzt2_game_data::{
    species::*,
    world_definitions::{document::WorldDefinitionDocument, simulation_time::*},
    AssetId,
};
use openzt2_world_state::simulation_time::deterministic_random_stream::{
    DeterministicRng, RngDomain, ZooSeed,
};
use openzt2_world_state::world_spawn::persistent_id_types::PersistentId;
use openzt2_zoo_state::animal_lifecycle::types::AnimalVariant;
use openzt2_zoo_state::behavior_task_execution_types::BehaviorTaskExecutionPhase;

fn document() -> SpeciesDocument {
    let id = AssetId::from_key("test_species");
    SpeciesDocument {
        species: vec![Species {
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
            flags: SpeciesFlags::ENDANGERED,
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
                    placement_surface_policy: Default::default(),
                    id: AssetId::from_key(&format!("young_{index}")),
                    behavior_subject_type_identifiers: vec![],
                    navigation_collision_policy: None,
                    ground_navigation_clip_asset_key: None,
                    medium_ground_navigation_clip_asset_key: None,
                    fast_ground_navigation_clip_asset_key: None,
                    surface_water_locomotion: None,
                    sex,
                    life_stage: LifeStage::Young,
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

fn install_species(app: &mut App, document: &SpeciesDocument) {
    let handle = app
        .world_mut()
        .resource_mut::<Assets<SpeciesAsset>>()
        .add(SpeciesAsset::from_test_document(document.clone()));
    app.world_mut()
        .resource_mut::<SpeciesAssets>()
        .index_test_document(handle, document);
}

#[test]
fn birth_action_spawns_one_young_variant_with_mother_and_mate() {
    let mut app = App::new();
    app.init_resource::<Assets<SpeciesAsset>>()
        .init_resource::<SpeciesAssets>()
        .init_resource::<Assets<WorldDefinitionAsset>>()
        .init_resource::<WorldDefinitions>()
        .init_resource::<Assets<BehaviorDocumentAsset>>()
        .insert_resource(ZooClock {
            fractional_tick: Default::default(),
            tick: 99,
            absolute_day: 0,
            tick_in_day: 99,
        })
        .add_message::<AnimalBorn>()
        .add_systems(Update, spawn_children_for_authored_births);
    let root = app.world_mut().spawn_empty().id();
    app.insert_resource(PersistentIdAllocator::new(root));
    let definitions = WorldDefinitionDocument {
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
        .add(WorldDefinitionAsset::from_test_document(
            definitions.clone(),
        ));
    app.world_mut()
        .resource_mut::<WorldDefinitions>()
        .index_test_document(handle, &definitions);
    install_species(&mut app, &document());
    let program = AssetId::from_key("SpawnM");
    let document = app
        .world_mut()
        .resource_mut::<Assets<BehaviorDocumentAsset>>()
        .add(BehaviorDocumentAsset::from_test_document(
            BehaviorDocument::Sets(vec![BehaviorSet {
                id: program,
                name: "SpawnM".into(),
                subjects: vec![],
                actions: BehaviorTaskActionPhase::Supported(vec![BehaviorAction::SpawnChild {
                    entity: AssetId::from_key("young_1"),
                    pose: BehaviorSpawnPosePolicy {
                        random_position_offset_m: 3.0,
                        random_rotation_offset_radians: 0.0,
                    },
                }]),
            }]),
        ));
    let father = app
        .world_mut()
        .spawn((Animal, PersistentId(10), WorldMember { root }))
        .id();
    let mother = app
        .world_mut()
        .spawn((
            Animal,
            WorldMember { root },
            SpeciesHandle {
                species: AssetId::from_key("test_species"),
            },
            Transform::from_xyz(3.0, 4.0, 5.0),
            Pregnancy { father: None },
            Mate(father),
            BehaviorRandomStream(DeterministicRng::from_entity(
                ZooSeed(1),
                PersistentId(20),
                RngDomain::Reproduction,
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
    app.update();
    let born = app
        .world_mut()
        .resource_mut::<Messages<AnimalBorn>>()
        .drain()
        .map(|birth| birth.child)
        .collect::<Vec<_>>();
    assert_eq!(born.len(), 1);
    let child = born[0];
    assert_eq!(
        *app.world().get::<Parents>(child).unwrap(),
        Parents {
            mother: Some(mother),
            father: Some(father),
        }
    );
    assert_eq!(
        app.world().get::<AnimalVariant>(child).unwrap().0,
        AssetId::from_key("young_1")
    );
    let offset =
        app.world().get::<Transform>(child).unwrap().translation - Vec3::new(3.0, 4.0, 5.0);
    assert!(offset.x.abs() <= 3.0 && offset.z.abs() <= 3.0);
    assert_eq!(
        app.world()
            .get::<BehaviorTaskExecutionState>(mother)
            .unwrap()
            .action,
        1
    );
}
