use bevy::prelude::*;
use openzt2_game_data::{
    world_definitions::{
        document::WorldDefinitionDocument,
        world_objects::{
            WorldObjectAffordanceFlags, WorldObjectDefinition, WorldObjectKind,
            WorldObjectNamedPhysicalPresentationDefinition, WorldObjectPropertyFlags,
        },
    },
    AssetId,
};

use super::{hydrate_required_named_physical_presentations, NamedPhysicalPresentationsHydrated};
use openzt2_game_assets::world_definitions::{
    world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions,
    world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset,
};
use crate::plugins::world_spawn::world_membership_types::DefinitionId;

fn presentation(
    name: &str,
    prefab: AssetId,
    initially_visible: bool,
) -> WorldObjectNamedPhysicalPresentationDefinition {
    WorldObjectNamedPhysicalPresentationDefinition {
        name: AssetId::from_key(name),
        prefab,
        scale: 1.0,
        required: true,
        initially_visible,
        attach_node: None,
        inherit_attach_rotation: false,
    }
}

fn marker_object(id: AssetId) -> WorldObjectDefinition {
    WorldObjectDefinition {
        supports_show_tricks: false,
        behavior_type_identifiers: Vec::new(),
        view_data: Vec::new(),
        detach_actions: Vec::new(),
        selected_ui_broadcasts: Vec::new(),
        information_panel: AssetId::default(),
        id,
        kind: WorldObjectKind::Scenery,
        view_class: None,
        biome_automatic_placement_class: None,
        name_key: AssetId::default(),
        description_key: AssetId::default(),
        zoopedia_subject: AssetId::default(),
        prefab: AssetId::default(),
        catalogue_preview_prefab: None,
        presentation_attachments: Vec::new(),
        // Without an asset server no scene is registered, so the second
        // binder stands in for a scene that never resolves.
        named_physical_presentations: vec![
            presentation("unfinishedEventObj", AssetId::default(), true),
            presentation(
                "missingSceneObj",
                AssetId::from_key("missing.nif#scene"),
                true,
            ),
            presentation("finishedEventObj", AssetId::default(), false),
        ],
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
        price_cents: 0,
        upkeep_cents_per_month: 0,
        properties: WorldObjectPropertyFlags::default(),
        affordances: WorldObjectAffordanceFlags::default(),
        destruction: None,
    }
}

#[test]
fn unresolved_binder_is_skipped_and_finished_binder_starts_hidden() {
    let definition = AssetId::from_key("gesture_event_marker");
    let mut application = App::new();
    application
        .insert_resource(Assets::<WorldDefinitionAsset>::default())
        .init_resource::<WorldDefinitions>()
        .add_systems(Update, hydrate_required_named_physical_presentations);
    let document = WorldDefinitionDocument {
        objects: vec![marker_object(definition)],
        ..default()
    };
    let handle = application
        .world_mut()
        .resource_mut::<Assets<WorldDefinitionAsset>>()
        .add(WorldDefinitionAsset::from_test_document(document.clone()));
    application
        .world_mut()
        .resource_mut::<WorldDefinitions>()
        .index_test_document(handle, &document);
    let owner = application.world_mut().spawn(DefinitionId(definition)).id();

    application.update();

    let world = application.world_mut();
    assert!(
        world
            .get::<NamedPhysicalPresentationsHydrated>(owner)
            .is_some(),
        "an unresolved scene must not leave the owner retrying every frame"
    );
    let mut children = world.query::<(&ChildOf, &Visibility)>();
    let visibility = children
        .iter(world)
        .filter(|(parent, _)| parent.parent() == owner)
        .map(|(_, visibility)| *visibility)
        .collect::<Vec<_>>();
    assert_eq!(visibility, [Visibility::Inherited, Visibility::Hidden]);
}
