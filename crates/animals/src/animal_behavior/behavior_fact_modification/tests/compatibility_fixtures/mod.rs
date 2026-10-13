use super::*;
use openzt2_game_assets::{
    species::species_asset_types::{SpeciesAsset, SpeciesAssets},
    world_definitions::{
        world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions,
        world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset,
    },
};
use openzt2_game_data::{
    species::*,
    world_definitions::{document::WorldDefinitionDocument, world_objects::*},
};
use openzt2_world_state::world_spawn::world_membership_types::*;
use openzt2_zoo_state::animal_lifecycle::types::*;
use openzt2_zoo_state::animal_welfare::types::*;

pub(super) fn install_definitions(
    app: &mut App,
    actor: Entity,
    target: Entity,
) -> (Handle<SpeciesAsset>, SpeciesDocument) {
    let species_id = AssetId::from_key("test:species");
    let variant_id = AssetId::from_key("test:variant");
    let document = SpeciesDocument {
        species: vec![Species {
            id: species_id,
            authored_species_name: "test:species".to_owned(),
            name_key: species_id,
            taxonomy_key: species_id,
            world_definition: species_id,
            needs: Vec::new(),
            habitat_space: None,
            adult_mass_kg: [0.0; 2],
            stage_start_days: [0.0; 4],
            lifespan_days: [0.0; 2],
            gestation_days: 0.0,
            litter: [0; 2],
            move_speed_mps: 0.0,
            swim_speed_mps: 0.0,
            appetite_per_day: 0.0,
            waste_definition: AssetId::default(),
            waste_interval_days: 0.0,
            waste_units: 0,
            keeper_food_definition: None,
            keeper_drink_definition: None,
            conservation: ConservationStatus::Unspecified,
            flags: SpeciesFlags::default(),
            adoption_offer: AnimalAdoptionOfferDefinition {
                rarity_fame_percent: 0,
                unlock_seconds_range: [0.0; 2],
                remove_after_seconds: 0.0,
                dismiss_cooldown_seconds: 0.0,
            },
        }],
        variants: vec![SpeciesVariantBinding {
            species: species_id,
            authored_species_name_override: None,
            variant: SpeciesVariant {
                id: variant_id,
                behavior_subject_type_identifiers: ["b_carnivore", "b_fooddisheater"]
                    .map(AssetId::from_key)
                    .to_vec(),
                navigation_collision_policy: None,
                ground_navigation_clip_asset_key: None,
                sex: Sex::Female,
                life_stage: LifeStage::Adult,
                model: String::new(),
                model_animation_set: AssetId::default(),
                initial_animation_clip_asset_key: None,
                initial_animation_loops: false,
                material_variant: None,
                scale: [1.0; 2],
                weight_kg: [0.0; 2],
                probability: 100,
                flags: SpeciesVariantFlags::default(),
                adoption_count_range: [1; 2],
            },
        }],
        compatibilities: Vec::new(),
    };
    app.init_resource::<Assets<SpeciesAsset>>()
        .init_resource::<SpeciesAssets>();
    let handle = app
        .world_mut()
        .resource_mut::<Assets<SpeciesAsset>>()
        .add(SpeciesAsset::from_test_document(document.clone()));
    app.world_mut()
        .resource_mut::<SpeciesAssets>()
        .index_test_document(handle.clone(), &document);
    app.world_mut().entity_mut(actor).insert((
        SpeciesHandle {
            species: species_id,
        },
        AnimalVariant(variant_id),
        AnimalSex(Sex::Female),
        AnimalLifeStage(LifeStage::Adult),
        GlobalTransform::default(),
        WorldMember { root: target },
    ));
    app.world_mut().entity_mut(actor).insert((
        Hunger::default(),
        Thirst::default(),
        RestNeed::default(),
        PrivacyNeed::default(),
        SocialNeed::default(),
        ExerciseNeed::default(),
        StimulationNeed::default(),
        EnvironmentNeed::default(),
        HealthNeed::default(),
        HygieneNeed::default(),
        BathroomNeed::default(),
        NeedTriggerState::default(),
    ));
    app.world_mut()
        .entity_mut(target)
        .insert(DefinitionId(AssetId::from_key("fooddish_meat")));
    let objects = [
        ("fooddish_meat", WorldObjectContainerContent::Food),
        ("fooddish_water", WorldObjectContainerContent::Drink),
    ]
    .into_iter()
    .map(|(name, content)| WorldObjectDefinition {
        id: AssetId::from_key(name),
        kind: WorldObjectKind::Food,
        supports_show_tricks: false,
        behavior_type_identifiers: Vec::new(),
        view_data: vec![],
        detach_actions: vec![],
        selected_ui_broadcasts: vec![],
        information_panel: AssetId::default(),
        view_class: None,
        biome_automatic_placement_class: None,
        name_key: AssetId::default(),
        description_key: AssetId::default(),
        zoopedia_subject: AssetId::default(),
        prefab: AssetId::default(),
        catalogue_preview_prefab: None,
        presentation_attachments: vec![],
        named_physical_presentations: vec![],
        interaction_slots: vec![],
        container_quantity: Some(WorldObjectContainerQuantityDefinition {
            initial_q16: 50 << 16,
            content,
            held_prop: None,
        }),
        transactions: vec![],
        prefab_scale: 1.0,
        icon: AssetId::default(),
        biomes: vec![],
        location: AssetId::default(),
        preview_offset_cm: [0; 3],
        preview_scale: 1.0,
        terrain_fitted: false,
        real_physics_water_impact: None,
        price_cents: 0,
        upkeep_cents_per_month: 0,
        properties: WorldObjectPropertyFlags::default(),
        affordances: WorldObjectAffordanceFlags::default(),
        destruction: None,
    })
    .collect();
    let world_document = WorldDefinitionDocument {
        objects,
        ..Default::default()
    };
    app.init_resource::<Assets<WorldDefinitionAsset>>()
        .init_resource::<WorldDefinitions>();
    let world_handle = app
        .world_mut()
        .resource_mut::<Assets<WorldDefinitionAsset>>()
        .add(WorldDefinitionAsset::from_test_document(
            world_document.clone(),
        ));
    app.world_mut()
        .resource_mut::<WorldDefinitions>()
        .index_test_document(world_handle, &world_document);
    (handle, document)
}
