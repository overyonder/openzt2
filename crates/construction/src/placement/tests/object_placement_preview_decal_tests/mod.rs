use bevy::prelude::*;

use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use openzt2_game_assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use openzt2_game_data::world_definitions::document::WorldDefinitionDocument;
use openzt2_game_data::world_definitions::object_placement::PlacementConstraints;
use openzt2_game_data::world_definitions::world_objects::WorldObjectDefinition;
use openzt2_game_data::world_definitions::world_objects::WorldObjectKind;
use openzt2_zoo_state::construction::construction_interaction_types::ConstructionPreview;
use openzt2_zoo_state::construction::construction_interaction_types::PlacementValidity;
use openzt2_zoo_state::placement::placement_preview_types::ObjectPlacementPreviewDecal;
use openzt2_zoo_state::placement::placement_preview_types::ObjectPlacementPreviewMaterials;

use super::create_object_placeable_definition_test_fixture;
use crate::placement::object_placement_preview_presentation::rebuild_object_placement_preview_footprint_and_grid_decals;

fn decals(app: &mut App) -> Vec<Entity> {
    let mut decals = app
        .world_mut()
        .query_filtered::<Entity, With<ObjectPlacementPreviewDecal>>()
        .iter(app.world())
        .collect::<Vec<_>>();
    decals.sort();
    decals
}

// #199: a preview whose draw inputs are unchanged keeps its decal entities.
#[test]
fn unchanged_placement_preview_keeps_its_footprint_and_grid_decals() {
    let mut app = App::new();
    app.init_resource::<Assets<WorldDefinitionAsset>>()
        .init_resource::<WorldDefinitions>()
        .init_resource::<Assets<Mesh>>()
        .add_systems(
            Update,
            rebuild_object_placement_preview_footprint_and_grid_decals,
        );
    let placeable =
        create_object_placeable_definition_test_fixture(PlacementConstraints::default());
    let definition = placeable.id;
    let document = WorldDefinitionDocument {
        objects: vec![WorldObjectDefinition {
            audio_cue_prefixes: Vec::new(),
            surface_effect_strength: None,
            presentation_caustic_policies: Vec::new(),
            id: definition,
            kind: WorldObjectKind::Scenery,
            supports_show_tricks: false,
            behavior_type_identifiers: Vec::new(),
            view_data: Vec::new(),
            detach_actions: Vec::new(),
            selected_ui_broadcasts: Vec::new(),
            information_panel: default(),
            view_class: None,
            biome_automatic_placement_class: None,
            name_key: default(),
            description_key: default(),
            zoopedia_subject: default(),
            prefab: default(),
            catalogue_preview_prefab: None,
            presentation_attachments: Vec::new(),
            named_physical_presentations: Vec::new(),
            interaction_slots: Vec::new(),
            container_quantity: None,
            transactions: Vec::new(),
            prefab_scale: 1.0,
            icon: default(),
            biomes: Vec::new(),
            location: default(),
            preview_offset_cm: [0; 3],
            preview_scale: 1.0,
            terrain_fitted: false,
            real_physics_water_impact: None,
            price_cents: 1_250,
            upkeep_cents_per_month: 0,
            properties: default(),
            affordances: default(),
            destruction: None,
        }],
        placeables: vec![placeable],
        ..default()
    };
    let handle = app
        .world_mut()
        .resource_mut::<Assets<WorldDefinitionAsset>>()
        .add(WorldDefinitionAsset::from_test_document(document.clone()));
    app.world_mut()
        .resource_mut::<WorldDefinitions>()
        .index_test_document(handle, &document);
    let preview = app
        .world_mut()
        .spawn((
            ConstructionPreview {
                definition,
                transform: Transform::default(),
                validity: PlacementValidity::Pending,
            },
            ObjectPlacementPreviewMaterials {
                footprint_valid: Handle::default(),
                footprint_invalid: Handle::default(),
                grid_cardinal: default(),
                grid_diagonal: default(),
                grid_radius: [1.0; 3],
                model_valid: Color::WHITE,
                model_invalid: Color::BLACK,
            },
        ))
        .id();

    app.update();
    let built = decals(&mut app);
    assert!(!built.is_empty());
    app.update();
    app.update();
    assert_eq!(decals(&mut app), built);

    // Changing a draw input replaces them.
    app.world_mut()
        .get_mut::<ConstructionPreview>(preview)
        .unwrap()
        .transform
        .rotate_y(std::f32::consts::FRAC_PI_2);
    app.update();
    let rebuilt = decals(&mut app);
    assert_eq!(rebuilt.len(), built.len());
    assert!(rebuilt.iter().all(|decal| !built.contains(decal)));
}
