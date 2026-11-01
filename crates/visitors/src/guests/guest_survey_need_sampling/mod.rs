use bevy::prelude::*;
use openzt2_game_assets::behavior::behavior_asset_types::BehaviorDocumentAsset;
use openzt2_game_assets::world_definitions::{
    world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions,
    world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset,
};
use openzt2_game_data::AssetId;
use openzt2_world_state::world_spawn::{
    world_entity_crating::WorldEntityIsCrated, world_membership_types::WorldMember,
};
use openzt2_zoo_state::guests::{
    guest_simulation_types::*,
    guest_survey_types::{GuestCriticalNeedObservations, GuestSurvey, GuestSurveyExperience},
};
use openzt2_zoo_state::{
    animal_behavior::behavior_set_start_request_types::PendingBehaviorSet,
    animal_health::types::{Dead, Frozen, Tranquilized},
    behavior_task_execution_types::{
        BehaviorTaskExecutionState, PendingBehaviorTaskFailure, SynchronizedBehaviorParticipant,
        find_current_behavior_task_action,
    },
};

pub(super) fn initialize_guest_survey_observations(
    mut commands: Commands,
    guests: Query<
        (
            Entity,
            Has<GuestCriticalNeedObservations>,
            Has<GuestSurveyExperience>,
        ),
        (
            With<Guest>,
            Or<(
                Without<GuestCriticalNeedObservations>,
                Without<GuestSurveyExperience>,
            )>,
        ),
    >,
) {
    for (guest, counters, experience) in &guests {
        let mut guest = commands.entity(guest);
        if !counters {
            guest.insert(GuestCriticalNeedObservations::default());
        }
        if !experience {
            guest.insert(GuestSurveyExperience::default());
        }
    }
}

#[allow(clippy::type_complexity)]
pub(super) fn sample_current_guest_need_observations(
    time: Res<Time<Fixed>>,
    definitions: Res<Assets<WorldDefinitionAsset>>,
    active: Res<WorldDefinitions>,
    behavior_documents: Res<Assets<BehaviorDocumentAsset>>,
    survey: Option<ResMut<GuestSurvey>>,
    thinking_guests: Query<
        (
            Option<&BehaviorTaskExecutionState>,
            Has<PendingBehaviorTaskFailure>,
        ),
        (
            With<WorldMember>,
            Without<Dead>,
            Without<Frozen>,
            Without<Tranquilized>,
            Without<WorldEntityIsCrated>,
            Without<SynchronizedBehaviorParticipant>,
            Without<PendingBehaviorSet>,
        ),
    >,
    mut guests: Query<
        (
            Entity,
            &GuestArchetype,
            &GuestHunger,
            &GuestThirst,
            &GuestDessert,
            &GuestGift,
            &GuestEnergy,
            &GuestRestroom,
            &GuestSocial,
            &GuestAmusement,
            &GuestSatisfaction,
            Option<&GuestViewingNeed>,
            &mut GuestCriticalNeedObservations,
            &mut GuestSurveyExperience,
        ),
        With<Guest>,
    >,
) {
    let Some(mut survey) = survey else {
        return;
    };
    let Some(definitions) = active.get(&definitions) else {
        return;
    };
    let Some(generation) = definitions.guest_generation() else {
        return;
    };
    if !definitions
        .behavior_selection_policy()
        .is_some_and(|policy| policy.enabled)
    {
        return;
    }
    let interval = generation.need_survey.check_interval_ns as f32 * 1e-9;
    for (
        entity,
        archetype,
        hunger,
        thirst,
        dessert,
        gift,
        rest,
        bathroom,
        social,
        amusement,
        happiness,
        viewing,
        mut counters,
        mut experience,
    ) in &mut guests
    {
        let Ok((task, pending_failure)) = thinking_guests.get(entity) else {
            continue;
        };
        if task.is_some_and(|task| {
            pending_failure
                || behavior_documents
                    .get(&task.document)
                    .and_then(|document| find_current_behavior_task_action(document, task))
                    .is_none()
        }) {
            continue;
        }
        let Some(definition) = definitions.find_guest(archetype.0) else {
            continue;
        };
        experience.observe_happiness(happiness.source_q16() as f32 / 65_536.0);
        if !survey.sample_guest_needs_now(time.timestep().as_secs_f32(), interval) {
            continue;
        }
        let values = [
            (hunger.value, hunger.residual_q16),
            (thirst.value, thirst.residual_q16),
            (dessert.value, dessert.residual_q16),
            (gift.value, gift.residual_q16),
            (rest.value, rest.residual_q16),
            (bathroom.value, bathroom.residual_q16),
            (social.value, social.residual_q16),
            (amusement.value, amusement.residual_q16),
        ];
        let names = [
            "hunger",
            "thirst",
            "dessert",
            "gift",
            "rest",
            "bathroom",
            "social",
            "amusement",
        ];
        for need in &definition.needs {
            let index = need.kind as usize;
            let (value, residual) = values[index];
            let source_q16 = (1_000_i64 << 16) - (i64::from(value) << 16) - i64::from(residual);
            if source_q16 > i64::from(need.critical_q16) * 10 {
                counters.observe(AssetId::from_key(names[index]));
            }
        }
        for (name, policy, value) in [
            (
                "happiness",
                definition.happiness_need,
                Some(happiness.source_q16()),
            ),
            (
                "viewanimals",
                definition.viewing_need,
                viewing.map(|need| need.value_q16),
            ),
        ] {
            if let (Some(policy), Some(value)) = (policy, value) {
                if policy
                    .critical_q16
                    .is_some_and(|threshold| value > threshold)
                {
                    counters.observe(AssetId::from_key(name));
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use openzt2_game_data::world_definitions::guest_simulation_definitions::{
        GuestNeedDefinition, GuestNeedKind,
    };
    use openzt2_world_state::simulation_time::simulation_clock_types::ZooClock;

    fn fixture() -> App {
        let mut app = App::new();
        let mut document = super::super::viewing_reaction_composition_tests::policy_document();
        document.behavior_selection_policy = Some(openzt2_game_data::world_definitions::behavior_selection_policy::BehaviorSelectionPolicy {
            enabled: true,
            minimum_score: 0.0,
            outside_range_need_value: 0.0,
            rejected_target_retry_interval: 1,
            rejected_target_retry_interval_cap: 1,
        });
        document
            .guest_generation
            .as_mut()
            .unwrap()
            .need_survey
            .check_interval_ns = 5_000_000_000;
        document.guests[0].needs = [
            GuestNeedKind::Hunger,
            GuestNeedKind::Thirst,
            GuestNeedKind::Dessert,
            GuestNeedKind::Gift,
            GuestNeedKind::Energy,
            GuestNeedKind::Restroom,
            GuestNeedKind::Social,
            GuestNeedKind::Amusement,
        ]
        .into_iter()
        .map(|kind| GuestNeedDefinition {
            kind,
            initial_permille: [500; 2],
            adjustment_q16: 0,
            reconsider_threshold: 0,
            cessation_threshold: None,
            critical_q16: match kind {
                GuestNeedKind::Hunger => 50 << 16,
                GuestNeedKind::Thirst => (50 << 16) - 1,
                _ => 101 << 16,
            },
        })
        .collect();
        app.init_resource::<Assets<WorldDefinitionAsset>>()
            .init_resource::<Assets<BehaviorDocumentAsset>>()
            .insert_resource(Time::<Fixed>::from_hz(10.0))
            .insert_resource(GuestSurvey {
                critical_need_countdown_seconds: 5.0,
                previous_day_of_month: 9,
                current_day_of_month: 1,
                ..Default::default()
            })
            .insert_resource(ZooClock {
                tick: 1,
                absolute_day: 0,
                tick_in_day: 1,
                fractional_tick: Default::default(),
            })
            .add_systems(
                Update,
                (
                    initialize_guest_survey_observations,
                    sample_current_guest_need_observations,
                )
                    .chain(),
            );
        let handle = app
            .world_mut()
            .resource_mut::<Assets<WorldDefinitionAsset>>()
            .add(WorldDefinitionAsset::from_test_document(document.clone()));
        let mut active = WorldDefinitions::default();
        active.index_test_document(handle, &document);
        app.insert_resource(active);
        app
    }

    #[test]
    fn empty_zoo_preserves_loaded_countdown_when_the_world_clock_advances() {
        let mut app = fixture();
        for _ in 0..20 {
            app.world_mut().resource_mut::<ZooClock>().tick += 1_000;
            app.update();
        }
        let survey = app.world().resource::<GuestSurvey>();
        assert_eq!(survey.critical_need_countdown_seconds, 5.0);
        assert_eq!(survey.previous_day_of_month, 9);
        assert_eq!(survey.current_day_of_month, 1);
    }

    #[test]
    fn a_guest_samples_individual_needs_with_a_strict_fractional_threshold() {
        let mut app = fixture();
        app.world_mut()
            .resource_mut::<GuestSurvey>()
            .critical_need_countdown_seconds = 0.0;
        let guest = app
            .world_mut()
            .spawn((
                Guest,
                WorldMember {
                    root: Entity::PLACEHOLDER,
                },
                GuestArchetype(AssetId::from_key("viewing-test")),
                GuestHunger {
                    value: 500,
                    residual_q16: 0,
                },
                GuestThirst {
                    value: 500,
                    residual_q16: 0,
                },
                GuestDessert {
                    value: 500,
                    residual_q16: 0,
                },
                GuestGift {
                    value: 500,
                    residual_q16: 0,
                },
                GuestEnergy {
                    value: 500,
                    residual_q16: 0,
                },
                GuestRestroom {
                    value: 500,
                    residual_q16: 0,
                },
                GuestSocial {
                    value: 500,
                    residual_q16: 0,
                },
                GuestAmusement {
                    value: 500,
                    residual_q16: 0,
                },
                GuestSatisfaction(500, 0),
            ))
            .id();
        app.update();
        let observations = app
            .world()
            .get::<GuestCriticalNeedObservations>(guest)
            .unwrap();
        assert_eq!(observations.observations(AssetId::from_key("hunger")), 0.0);
        assert_eq!(observations.observations(AssetId::from_key("thirst")), 1.0);
        assert_eq!(
            app.world()
                .resource::<GuestSurvey>()
                .critical_need_countdown_seconds,
            5.0
        );
        app.update();
        assert_eq!(
            app.world()
                .get::<GuestCriticalNeedObservations>(guest)
                .unwrap()
                .observations(AssetId::from_key("thirst")),
            1.0
        );
        app.world_mut().entity_mut(guest).insert(Frozen);
        let before = *app.world().resource::<GuestSurvey>();
        let experience = *app.world().get::<GuestSurveyExperience>(guest).unwrap();
        app.update();
        assert_eq!(*app.world().resource::<GuestSurvey>(), before);
        assert_eq!(
            *app.world().get::<GuestSurveyExperience>(guest).unwrap(),
            experience
        );
    }
}
