use super::*;
use openzt2_game_data::{
    species::*,
    world_definitions::{document::WorldDefinitionDocument, simulation_time::*},
    AssetId,
};
use openzt2_zoo_state::animal_lifecycle::types::AnimalSex;
use openzt2_zoo_state::animal_lifecycle::types::AnimalVariant;

fn document() -> SpeciesDocument {
    let id = AssetId::from_key("test_species");
    SpeciesDocument {
        species: vec![Species {
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

fn setup(entity_padding: usize) -> (App, Entity, Entity) {
    let mut app = App::new();
    app.init_resource::<Assets<SpeciesAsset>>()
        .init_resource::<SpeciesAssets>()
        .init_resource::<Assets<WorldDefinitionAsset>>()
        .init_resource::<WorldDefinitions>()
        .insert_resource(ZooSeed(123))
        .insert_resource(ZooClock {
            tick: 99,
            absolute_day: 0,
            tick_in_day: 99,
        })
        .add_message::<AnimalBorn>()
        .add_systems(
            Update,
            (
                complete_due_animal_pregnancies,
                super::super::animal_lineage_reference_cleanup::
                    clear_lineage_references_to_deleted_animal_entities,
            )
                .chain(),
        );
    for _ in 0..entity_padding {
        app.world_mut().spawn_empty();
    }
    let root = app.world_mut().spawn_empty().id();
    let mut allocator = PersistentIdAllocator::new(root);
    allocator.reserve_imported(root, PersistentId(20)).unwrap();
    app.insert_resource(allocator);
    let father = app
        .world_mut()
        .spawn((Animal, PersistentId(10), WorldMember { root }))
        .id();
    let mother = app
        .world_mut()
        .spawn((
            Animal,
            PersistentId(20),
            WorldMember { root },
            SpeciesHandle {
                species: AssetId::from_key("test_species"),
            },
            Pregnancy {
                father,
                due_tick: 100,
            },
            ReproductionTraits {
                gestation_ticks: 50,
                litter: [2, 4],
            },
            Transform::from_xyz(3.0, 4.0, 5.0).with_scale(Vec3::splat(8.0)),
        ))
        .id();
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
    (app, mother, father)
}

fn born(app: &mut App) -> Vec<Entity> {
    app.world_mut()
        .resource_mut::<Messages<AnimalBorn>>()
        .drain()
        .map(|birth| birth.child)
        .collect()
}

#[test]
fn reconstructed_pregnancy_uses_persistent_identity_and_due_tick_for_randomness() {
    let outcomes = [0, 17].map(|padding| {
        let (mut app, _, _) = setup(padding);
        app.world_mut().resource_mut::<ZooClock>().tick = 100;
        app.update();
        born(&mut app)
            .into_iter()
            .map(|child| {
                (
                    *app.world().get::<PersistentId>(child).unwrap(),
                    *app.world().get::<AnimalSex>(child).unwrap(),
                    *app.world().get::<AnimalVariant>(child).unwrap(),
                    app.world().get::<Transform>(child).unwrap().scale,
                )
            })
            .collect::<Vec<_>>()
    });
    assert!((2..=4).contains(&outcomes[0].len()));
    assert_eq!(outcomes[0], outcomes[1]);
}
