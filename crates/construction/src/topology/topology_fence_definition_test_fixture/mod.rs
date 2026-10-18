use bevy::prelude::*;
use openzt2_game_data::world_definitions::document::WorldDefinitionDocument;
use openzt2_game_data::world_definitions::fences_and_gates::FenceDefinition;
use openzt2_game_data::world_definitions::fences_and_gates::FenceGatePolicy;
use openzt2_game_data::world_definitions::fences_and_gates::FenceSegmentPrefabs;
use openzt2_game_data::world_definitions::fences_and_gates::FenceTraversalBlockingFlags;
use openzt2_game_data::world_definitions::world_objects::WorldObjectAffordanceFlags;
use openzt2_game_data::world_definitions::world_objects::WorldObjectDefinition;
use openzt2_game_data::world_definitions::world_objects::WorldObjectKind;
use openzt2_game_data::world_definitions::world_objects::WorldObjectPropertyFlags;
use openzt2_game_data::AssetId;

use openzt2_game_assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;

pub(super) fn install_fence_definition_for_topology_transaction_tests(
    app: &mut App,
    definition: AssetId,
) {
    let object = AssetId([42; 16]);
    let document = WorldDefinitionDocument {
        objects: vec![WorldObjectDefinition {
            audio_cue_prefixes: Vec::new(),
            surface_effect_strength: None,
            presentation_caustic_policies: Vec::new(),
            supports_show_tricks: false,
            behavior_type_identifiers: Vec::new(),
            view_data: Vec::new(),
            detach_actions: Vec::new(),
            selected_ui_broadcasts: Vec::new(),
            information_panel: AssetId::from_key("Scenery Info"),
            id: object,
            kind: WorldObjectKind::Fence,
            view_class: None,
            biome_automatic_placement_class: None,
            name_key: AssetId::default(),
            description_key: AssetId::default(),
            zoopedia_subject: AssetId::default(),
            prefab: AssetId::default(),
            catalogue_preview_prefab: None,
            presentation_attachments: Vec::new(),
            named_physical_presentations: Vec::new(),
            interaction_slots: Vec::new(),
            container_quantity: None,
            transactions: Vec::new(),
            prefab_scale: 1.0,
            icon: AssetId::default(),
            biomes: Vec::new(),
            location: AssetId::default(),
            preview_offset_cm: [0; 3],
            preview_scale: 1.0,
            terrain_fitted: false,
            real_physics_water_impact: None,
            price_cents: 100,
            upkeep_cents_per_month: 0,
            properties: WorldObjectPropertyFlags::default(),
            affordances: WorldObjectAffordanceFlags::default(),
            destruction: None,
        }],
        fences: vec![FenceDefinition {
            fitting_curves: Vec::new(),
            id: definition,
            object,
            segment_length_cm: 100,
            height_cm: 100,
            strength: 1,
            blocks: FenceTraversalBlockingFlags::default(),
            gate: AssetId::default(),
            post_prefab: AssetId::default(),
            segments: FenceSegmentPrefabs {
                cardinal_straight: AssetId::from_key("test/cardinal_straight"),
                diagonal_straight: AssetId::from_key("test/diagonal_straight"),
                cardinal_curve_90: AssetId::from_key("test/cardinal_curve_90"),
                diagonal_curve_90: AssetId::from_key("test/diagonal_curve_90"),
                cardinal_curve_135: AssetId::from_key("test/cardinal_curve_135"),
                diagonal_curve_135: AssetId::from_key("test/diagonal_curve_135"),
            },
            gate_policy: FenceGatePolicy::default(),
        }],
        ..default()
    };
    let handle = app
        .world_mut()
        .resource_mut::<Assets<WorldDefinitionAsset>>()
        .add(WorldDefinitionAsset::from_test_document(document.clone()));
    app.world_mut()
        .resource_mut::<WorldDefinitions>()
        .index_test_document(handle, &document);
}
