use bevy::prelude::*;
use openzt2_game_data::{
    world_definitions::{
        document::WorldDefinitionDocument,
        fences_and_gates::{
            FenceDefinition, FenceGatePolicy, FenceSegmentPrefabs, FenceTraversalBlockingFlags,
        },
        world_objects::{
            WorldObjectAffordanceFlags, WorldObjectDefinition, WorldObjectKind,
            WorldObjectPropertyFlags,
        },
    },
    AssetId,
};

use crate::assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use crate::assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;

pub(super) fn install_fence_definition_for_topology_transaction_tests(
    app: &mut App,
    definition: AssetId,
) {
    let object = AssetId([42; 16]);
    let document = WorldDefinitionDocument {
        objects: vec![WorldObjectDefinition {
            supports_show_tricks: false,
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
            id: definition,
            object,
            segment_length_cm: 100,
            height_cm: 100,
            strength: 1,
            blocks: FenceTraversalBlockingFlags::default(),
            gate: AssetId::default(),
            post_prefab: AssetId::default(),
            segments: FenceSegmentPrefabs {
                cardinal_straight: AssetId::default(),
                diagonal_straight: AssetId::default(),
                cardinal_curve_90: AssetId::default(),
                diagonal_curve_90: AssetId::default(),
                cardinal_curve_135: AssetId::default(),
                diagonal_curve_135: AssetId::default(),
            },
            gate_policy: FenceGatePolicy {
                prefab: AssetId::default(),
                open_animation: AssetId::default(),
                close_animation: AssetId::default(),
                trigger_distance_cm: 0,
                auto_close_ticks: 0,
            },
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
