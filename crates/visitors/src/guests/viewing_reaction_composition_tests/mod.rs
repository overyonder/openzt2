use super::{
    guest_lifecycle_and_memory_execution::apply_guest_reactions, guest_simulation_types::*,
    view_event_execution::BehaviorViewEventEmitted,
};
use bevy::prelude::*;
use openzt2_game_assets::world_definitions::{
    world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions,
    world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset,
};
use openzt2_game_data::{
    world_definitions::{
        document::WorldDefinitionDocument, guest_simulation_definitions::*, world_objects::*,
    },
    AssetId,
};
use openzt2_world_state::world_spawn::world_membership_types::DefinitionId;
use openzt2_world_state::world_spawn::world_membership_types::WorldMember;

fn policy_document() -> WorldDefinitionDocument {
    let id = AssetId::from_key("viewing-test");
    WorldDefinitionDocument {
        objects: vec![WorldObjectDefinition {
            audio_cue_prefixes: Vec::new(),
            surface_effect_strength: None,
            presentation_caustic_policies: Vec::new(),
            id,
            kind: WorldObjectKind::Scenery,
            supports_show_tricks: false,
            behavior_type_identifiers: Vec::new(),
            view_data: vec![WorldObjectViewData {
                name: id,
                view_score: 1.0,
                tour_score: 0.0,
                donation_score: 0.0,
                entertainment_score: 0.0,
            }],
            detach_actions: vec![],
            selected_ui_broadcasts: vec![],
            information_panel: id,
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
            properties: Default::default(),
            affordances: Default::default(),
            destruction: None,
        }],
        guests: vec![GuestDefinition {
            donation: None,
            id,
            object: id,
            behavior_subject_type_identifiers: vec![],
            model_animation_set: None,
            stationary_animation_graph_node: None,
            clothing_materials: [None, None],
            interaction_slots: Vec::new(),
            name_pool: id,
            rarity: GuestRarity::Common,
            preferred_animal_view_factor_q16: 65536,
            view_meter_decay_per_second: 1.0,
            view_scoring: Default::default(),
            starting_cash_cents: [0; 2],
            initial_departure_points_q16: 0,
            radius_cm: 25,
            move_speed_mps: 1.0,
            patience_ticks: 20,
            needs: vec![],
            viewing_need: None,
            happiness_need: None,
            destination_weights: vec![],
            reactions: vec![GuestReactionDefinition {
                kind: GuestMemoryKind::AnimalView,
                satisfaction_delta: 20,
                education_delta: 30,
                retention_ticks: 100,
            }],
            memory: GuestMemoryPolicy {
                capacity: 8,
                retention_ticks: 100,
                replacement: MemoryReplacement::Oldest,
            },
        }],
        guest_generation: Some(GuestGenerationPolicy {
            enabled: false,
            use_test_type: false,
            maximum_guests_base: 0,
            maximum_guests_per_half_star: 0,
            spawn_at_entrance: false,
            arrival_offset_m: [0.0; 2],
            departure_offset_m: [0.0; 2],
            delay_ns: [0; 2],
            default_emitter_m: [0.0; 2],
            stagger: false,
            stagger_square_m: 0.0,
            base_spawn_probability: 0.0,
            minimum_spawn_rate: 0.0,
            normal_admission_cents: [0; 2],
            fame_rate_increase_permille_per_half_star: 0,
            rarity_thresholds: [0; 2],
            price_adjustments: vec![],
            species_adjustment: GuestSpeciesAdjustment {
                species_per_adjustment: 1,
                percent_per_adjustment: 0.0,
                maximum_percent_adjustment: 0.0,
            },
            need_survey: GuestNeedSurveyPolicy {
                check_interval_ns: 1,
                maximum_critical_hits_per_month: 100,
                maximum_education_points_per_view: 25,
                maximum_entertainment_points_per_view: 25,
                monitored: Default::default(),
            },
        }),
        ..default()
    }
}

fn install_policy(app: &mut App, document: &WorldDefinitionDocument) {
    let handle = app
        .world_mut()
        .resource_mut::<Assets<WorldDefinitionAsset>>()
        .add(WorldDefinitionAsset::from_test_document(document.clone()));
    let mut definitions = WorldDefinitions::default();
    definitions.index_test_document(handle, document);
    app.insert_resource(definitions);
}

fn fixture() -> (App, Entity, Entity, Entity) {
    let mut app = App::new();
    app.init_resource::<Assets<WorldDefinitionAsset>>()
        .init_resource::<Assets<openzt2_game_assets::behavior::behavior_asset_types::BehaviorDocumentAsset>>()
        .init_resource::<GuestSurvey>()
        .add_message::<GuestReaction>()
        .add_message::<GuestReactionApplied>()
        .add_message::<BehaviorViewEventEmitted>()
        .add_systems(Update, apply_guest_reactions);
    install_policy(&mut app, &policy_document());
    let root = app.world_mut().spawn_empty().id();
    let area = app.world_mut().spawn(WorldMember { root }).id();
    let subject = app
        .world_mut()
        .spawn((
            WorldMember { root },
            DefinitionId(AssetId::from_key("viewing-test")),
        ))
        .id();
    let mut spawn_guest = || {
        app.world_mut()
            .spawn((
                Guest,
                GuestArchetype(AssetId::from_key("viewing-test")),
                GuestPhase::Visiting,
                WorldMember { root },
                GuestSatisfaction(500, 0),
                GuestEducation(0),
                GuestMemories::new(8),
                Viewing {
                    subject,
                    area: Some(area),
                    started_tick: 10,
                    task_execution_id: None,
                },
                GuestDestination {
                    entity: area,
                    purpose: GuestVisitPurpose::View,
                },
                GuestNavigationRequest(None),
            ))
            .id()
    };
    let first = spawn_guest();
    let second = spawn_guest();
    (app, subject, first, second)
}

// Synthetic recipient delivery: no production visibility/meter execution is asserted.
fn request(app: &App, guest: Entity, event_id: u64) -> GuestReaction {
    let viewing = *app.world().get::<Viewing>(guest).unwrap();
    GuestReaction {
        view_event: Some(GuestViewReactionOrigin {
            event_id,
            source_definition: AssetId::from_key("viewing-test"),
            view_key: AssetId::from_key("viewing-test"),
            viewing,
        }),
        guest,
        subject: viewing.subject,
        kind: GuestMemoryKind::AnimalView,
        satisfaction_delta_permille: 999,
        education_delta_permille: 999,
    }
}

fn emit(app: &mut App, subject: Entity, event_id: u64) {
    let id = AssetId::from_key("viewing-test");
    app.world_mut().write_message(BehaviorViewEventEmitted {
        event_id,
        source: subject,
        source_definition: id,
        view_key: id,
    });
}

#[test]
fn reversed_reaction_delivery_preserves_distinct_event_effects() {
    let (mut app, subject, guest, _) = fixture();
    emit(&mut app, subject, 1);
    emit(&mut app, subject, 2);
    let first = request(&app, guest, 1);
    let second = request(&app, guest, 2);
    app.world_mut().write_message(second);
    app.world_mut().write_message(first);
    app.update();
    assert_eq!(app.world().get::<GuestEducation>(guest).unwrap().0, 50);
    assert_eq!(app.world().resource::<GuestSurvey>().education_points, 50);
    // Starting another observation does not make an already applied event new.
    {
        let mut viewing = *app.world().get::<Viewing>(guest).unwrap();
        viewing.started_tick += 1;
        app.world_mut().entity_mut(guest).insert(viewing);
    };
    emit(&mut app, subject, 2);
    let replay = request(&app, guest, 2);
    app.world_mut().write_message(replay);
    app.update();
    assert_eq!(app.world().get::<GuestEducation>(guest).unwrap().0, 50);
}

#[test]
fn an_emitted_view_event_reaches_every_current_viewer_once() {
    let (mut app, subject, first, second) = fixture();
    app.add_systems(
        Update,
        super::view_event_execution::deliver_view_events_to_current_viewers
            .before(apply_guest_reactions),
    );
    emit(&mut app, subject, 1);
    app.update();
    assert_eq!(app.world().get::<GuestEducation>(first).unwrap().0, 25);
    assert_eq!(app.world().get::<GuestEducation>(second).unwrap().0, 25);
    // A guest who stopped watching is not reached by the next event.
    app.world_mut().entity_mut(second).remove::<Viewing>();
    emit(&mut app, subject, 2);
    app.update();
    assert_eq!(app.world().get::<GuestEducation>(first).unwrap().0, 50);
    assert_eq!(app.world().get::<GuestEducation>(second).unwrap().0, 25);
}

#[test]
fn a_guest_watching_through_its_view_task_receives_the_event() {
    use openzt2_game_assets::behavior::behavior_asset_types::BehaviorDocumentAsset;
    use openzt2_game_data::behavior::{
        action::view_animals::BehaviorViewAnimalsAction,
        action_record::BehaviorAction,
        document::{BehaviorDocument, BehaviorSet, BehaviorTaskActionPhase},
    };

    let (mut app, subject, guest, _) = fixture();
    app.add_systems(
        Update,
        super::view_event_execution::deliver_view_events_to_current_viewers
            .before(apply_guest_reactions),
    );
    let area = app.world().get::<Viewing>(guest).unwrap().area;
    let document = app
        .world_mut()
        .resource_mut::<Assets<BehaviorDocumentAsset>>()
        .add(BehaviorDocumentAsset::from_test_document(
            BehaviorDocument::Sets(vec![BehaviorSet {
                id: AssetId::default(),
                name: "viewing".into(),
                subjects: Vec::new(),
                actions: BehaviorTaskActionPhase::Supported(vec![BehaviorAction::ViewAnimals(
                    BehaviorViewAnimalsAction {
                        view_meter_start: 60.0,
                        view_meter_start_range: [60.0; 2],
                        dock_viewpoint_behavior_set: None,
                        retarget: None,
                        emote_sets: Vec::new(),
                    },
                )]),
            }]),
        ));
    app.world_mut()
        .entity_mut(guest)
        .remove::<(GuestDestination, GuestNavigationRequest)>()
        .insert(
            openzt2_zoo_state::behavior_task_execution_types::BehaviorTaskExecutionState {
                execution_id: 1,
                program: AssetId::default(),
                target: area,
                document,
                declaration: 0,
                phase: openzt2_zoo_state::behavior_task_execution_types::BehaviorTaskExecutionPhase::Set,
                action: 0,
                repetitions: 0,
                next_action_tick: 0,
                object: None,
                interaction_slot: None,
                stack: default(),
            },
        );
    let mut viewing = *app.world().get::<Viewing>(guest).unwrap();
    viewing.task_execution_id = Some(1);
    app.world_mut().entity_mut(guest).insert(viewing);
    emit(&mut app, subject, 1);
    app.update();
    assert_eq!(app.world().get::<GuestEducation>(guest).unwrap().0, 25);
    app.world_mut()
        .get_mut::<openzt2_zoo_state::behavior_task_execution_types::BehaviorTaskExecutionState>(
            guest,
        )
        .unwrap()
        .execution_id = 2;
    emit(&mut app, subject, 2);
    app.update();
    assert_eq!(app.world().get::<GuestEducation>(guest).unwrap().0, 25);
    let mut viewing = *app.world().get::<Viewing>(guest).unwrap();
    viewing.task_execution_id = Some(2);
    app.world_mut().entity_mut(guest).insert(viewing);
    emit(&mut app, subject, 3);
    app.update();
    assert_eq!(app.world().get::<GuestEducation>(guest).unwrap().0, 50);
}
