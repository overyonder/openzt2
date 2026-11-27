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
use openzt2_world_state::world_spawn::world_membership_types::DefinitionId;

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
        fade: None,
    }
}

fn marker_object(id: AssetId) -> WorldObjectDefinition {
    WorldObjectDefinition {
        collision_scores: None,
        authored_runtime: Default::default(),
        audio_cue_prefixes: Vec::new(),
        audio_stage_volumes: Vec::new(),
        surface_effect_strength: None,
        presentation_caustic_policies: Vec::new(),
        traversal_animation_manifests: Vec::new(),
        water_filter: None,
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
        terrain_fit_height_offset_m: 0.0,
        terrain_fit_water_offset_m: None,
        real_physics_water_impact: None,
        price_cents: 0,
        upkeep_cents_per_month: 0,
        properties: WorldObjectPropertyFlags::default(),
        affordances: WorldObjectAffordanceFlags::default(),
        destruction: None,
        virtual_nodes: Vec::new(),
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

#[test]
fn selects_authored_fade_states_from_a_named_float_value() {
    use super::select_named_presentation_fade_states;
    use openzt2_game_data::world_definitions::world_objects::WorldObjectFadeRange;
    use openzt2_zoo_state::authored_float_attributes::AuthoredFloatAttributes;

    let definition = AssetId::from_key("insects");
    let mut object = marker_object(definition);
    let attribute = AssetId::from_key("f_custom_meter");
    object.named_physical_presentations = [("full", 50.0, 101.0), ("half", 1.0, 50.0)]
        .into_iter()
        .enumerate()
        .map(|(state_ordinal, (name, minimum, maximum))| {
            WorldObjectNamedPhysicalPresentationDefinition {
                fade: Some(WorldObjectFadeRange {
                    attribute,
                    minimum,
                    maximum,
                    controller_ordinal: 0,
                    state_ordinal: state_ordinal as u32,
                    fade_period_seconds: 0.0,
                    max_alpha: 1.0,
                    fade_hold: false,
                    wrap_value: false,
                }),
                ..presentation(name, AssetId::default(), name == "full")
            }
        })
        .collect();
    let mut application = App::new();
    application
        .insert_resource(Assets::<WorldDefinitionAsset>::default())
        .init_resource::<WorldDefinitions>()
        .add_systems(
            Update,
            (
                hydrate_required_named_physical_presentations,
                select_named_presentation_fade_states,
            )
                .chain(),
        );
    let document = WorldDefinitionDocument {
        objects: vec![object],
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
    let value = |points: f32| {
        let mut values = AuthoredFloatAttributes::default();
        values.set(attribute, points);
        values
    };
    let owner = application
        .world_mut()
        .spawn((DefinitionId(definition), value(100.0)))
        .id();
    let visibility = |application: &mut App| {
        let world = application.world_mut();
        let mut children = world.query::<(&ChildOf, &Visibility)>();
        children
            .iter(world)
            .filter(|(parent, _)| parent.parent() == owner)
            .map(|(_, visibility)| *visibility)
            .collect::<Vec<_>>()
    };

    application.update();
    application.update();
    assert_eq!(
        visibility(&mut application),
        [Visibility::Inherited, Visibility::Hidden]
    );
    application
        .world_mut()
        .entity_mut(owner)
        .insert(value(20.0));
    application.update();
    assert_eq!(
        visibility(&mut application),
        [Visibility::Hidden, Visibility::Inherited]
    );
    // Values outside every state range leave the current state selected.
    application.world_mut().entity_mut(owner).insert(value(0.0));
    application.update();
    assert_eq!(
        visibility(&mut application),
        [Visibility::Hidden, Visibility::Inherited]
    );
}
