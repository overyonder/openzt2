use avian3d::prelude::{SpatialQuery, SpatialQueryFilter};
use bevy::prelude::*;
use openzt2_game_data::AssetId;

use crate::assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use crate::assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use crate::plugins::input::input_types::ActionRequest;
use crate::plugins::input::input_types::GameAction;
use crate::plugins::animal_health::tranquilizer_eligibility_calculation::authored_tranquilizer_allows_animal_state;
use crate::plugins::animal_health::types::AimTranquilizer;
use crate::plugins::animal_health::types::Dead;
use crate::plugins::animal_health::types::Escaped;
use crate::plugins::animal_health::types::FireTranquilizer;
use crate::plugins::animal_health::types::Rampaging;
use crate::plugins::animal_health::types::TranquilizerTool;
use crate::plugins::animal_lifecycle::types::Animal;
use crate::plugins::audio::audio_playback_message_types::PlayAudioCue;
use crate::plugins::audio::audio_playback_message_types::StopAudioCue;
use crate::plugins::camera::camera_runtime_state_types::ZooCamera;
use crate::plugins::ui::ui_document_lifecycle_contracts::HideUiDocument;

use super::animal_care_control_types::{
    AnimalCareTarget, TranquilizerAudioFeedback, TranquilizerControl,
};

pub(super) fn initialize_tranquilizer_tool_from_active_world_definitions(
    mut commands: Commands,
    world_definition_assets: Res<Assets<WorldDefinitionAsset>>,
    active_world_definitions: Res<WorldDefinitions>,
    added_tranquilizer_controls: Query<(Entity, &AnimalCareTarget), Added<TranquilizerControl>>,
) {
    if added_tranquilizer_controls.is_empty() {
        return;
    }
    let Some(world_definitions) = active_world_definitions.get(&world_definition_assets) else {
        return;
    };
    let Some(tranquilizer_definition) = world_definitions.tranquilizers().next() else {
        return;
    };
    let Some(tranquilizer_mode_policy) = world_definitions.tranquilizer_mode() else {
        return;
    };
    for (controller_entity, animal_care_target) in &added_tranquilizer_controls {
        commands.entity(controller_entity).insert((
            TranquilizerTool {
                target: animal_care_target.animal_entity,
                tranquilizer: AssetId(tranquilizer_definition.id.0),
                charge_points: 0.0,
                required_charge_points: tranquilizer_mode_policy.required_charge_points,
            },
            TranquilizerAudioFeedback::default(),
        ));
    }
}

/// Uses Avian's ECS-native spatial query as the sole world picking authority.
/// The selected animal is written directly onto the controller's tool.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub(super) fn aim_charge_fire_or_cancel_active_tranquilizer_controls(
    time: Res<Time>,
    spatial_query: SpatialQuery,
    parents: Query<&ChildOf>,
    animals: Query<
        (&GlobalTransform, Option<&Escaped>, Option<&Rampaging>),
        (With<Animal>, Without<Dead>),
    >,
    zoo_cameras: Query<&GlobalTransform, With<ZooCamera>>,
    world_definition_assets: Res<Assets<WorldDefinitionAsset>>,
    active_world_definitions: Res<WorldDefinitions>,
    pointer_input: (
        Res<crate::plugins::input::input_types::PrimaryPointerInputState>,
        Res<crate::plugins::ui::active_authored_ui_context::AuthoredModalInputCapture>,
    ),
    mut tranquilizer_controls: Query<
        (
            Entity,
            &mut TranquilizerTool,
            &mut TranquilizerAudioFeedback,
        ),
        With<TranquilizerControl>,
    >,
    mut action_requests: MessageReader<ActionRequest>,
    mut aim_requests: MessageWriter<AimTranquilizer>,
    mut fire_requests: MessageWriter<FireTranquilizer>,
    mut play_audio_cue_requests: MessageWriter<PlayAudioCue>,
    mut stop_audio_cue_requests: MessageWriter<StopAudioCue>,
    mut commands: Commands,
    mut hide_ui_document_requests: MessageWriter<HideUiDocument>,
) {
    let (pointer_input, modal_input) = pointer_input;
    if modal_input.0.is_some() {
        action_requests.clear();
        // Losing input capture is not a trigger release. Keep the tool's
        // gameplay charge under its existing pause/lifetime owner, but retire
        // the physical hold and its looping audio so closing a dialog cannot
        // turn a release that happened inside the UI into a delayed shot.
        for (controller, _, mut audio_feedback) in &mut tranquilizer_controls {
            if audio_feedback.trigger_is_held {
                stop_audio_cue_requests.write(StopAudioCue {
                    emitter: Some(controller),
                    selection: controller.to_bits().wrapping_add(1),
                });
            }
            audio_feedback.trigger_is_held = false;
        }
        return;
    }
    if tranquilizer_controls.is_empty() {
        return;
    }
    let Ok(zoo_camera_transform) = zoo_cameras.single() else {
        return;
    };
    let Some(world_definitions) = active_world_definitions.get(&world_definition_assets) else {
        return;
    };
    let Some(active_tranquilizer_definition) = world_definitions.tranquilizers().next() else {
        return;
    };
    let Some(tranquilizer_mode_policy) = world_definitions.tranquilizer_mode() else {
        return;
    };
    let targeted_animal_entity = spatial_query
        .cast_ray_predicate(
            zoo_camera_transform.translation(),
            zoo_camera_transform.forward(),
            f32::MAX,
            false,
            &SpatialQueryFilter::DEFAULT,
            &|entity| {
                find_animal_ancestor(entity, &parents, &animals).is_some_and(|animal_entity| {
                    animals
                        .get(animal_entity)
                        .is_ok_and(|(_, escaped, rampaging)| {
                            authored_tranquilizer_allows_animal_state(
                                active_tranquilizer_definition.eligible_states,
                                escaped.is_some(),
                                rampaging.is_some(),
                            )
                        })
                })
            },
        )
        .and_then(|hit| find_animal_ancestor(hit.entity, &parents, &animals));

    for (controller_entity, mut tranquilizer_tool, mut audio_feedback) in &mut tranquilizer_controls
    {
        if world_definitions
            .find_tranquilizer(tranquilizer_tool.tranquilizer)
            .is_none()
        {
            continue;
        }
        tranquilizer_tool.target = targeted_animal_entity;
        let targeted_animal_is_in_range = targeted_animal_entity
            .and_then(|animal_entity| animals.get(animal_entity).ok())
            .is_some_and(|(animal_transform, _, _)| {
                zoo_camera_transform
                    .translation()
                    .distance(animal_transform.translation())
                    <= tranquilizer_mode_policy.range_cm as f32 / 100.0
            });
        let trigger_is_charging = targeted_animal_is_in_range && pointer_input.pressed;
        let charge_audio_selection = controller_entity.to_bits().wrapping_add(1);
        let charge_rate_points_per_second = if trigger_is_charging {
            tranquilizer_mode_policy.charge_points_per_second
        } else {
            -tranquilizer_mode_policy.charge_decrease_points_per_second
        };
        tranquilizer_tool.charge_points = (tranquilizer_tool.charge_points
            + time.delta_secs() * charge_rate_points_per_second)
            .clamp(0.0, tranquilizer_tool.required_charge_points);

        if trigger_is_charging
            && (targeted_animal_entity != audio_feedback.targeted_animal_entity
                || !audio_feedback.trigger_is_held)
        {
            if audio_feedback.trigger_is_held {
                stop_audio_cue_requests.write(StopAudioCue {
                    emitter: Some(controller_entity),
                    selection: charge_audio_selection,
                });
            }
            let begin_or_lost_target_cue = targeted_animal_entity.map_or(
                AssetId(tranquilizer_mode_policy.lose_tracking_cue.0),
                |_| AssetId(tranquilizer_mode_policy.begin_charge_cue.0),
            );
            play_audio_cue_requests.write(PlayAudioCue {
                cue: begin_or_lost_target_cue,
                emitter: Some(controller_entity),
                selection: controller_entity.to_bits(),
                priority: 192,
                force_looped: false,
            });
            play_audio_cue_requests.write(PlayAudioCue {
                cue: AssetId(tranquilizer_mode_policy.charge_cue.0),
                emitter: Some(controller_entity),
                selection: charge_audio_selection,
                priority: 191,
                force_looped: true,
            });
        } else if !trigger_is_charging && audio_feedback.trigger_is_held {
            stop_audio_cue_requests.write(StopAudioCue {
                emitter: Some(controller_entity),
                selection: charge_audio_selection,
            });
        }
        if targeted_animal_entity.is_none() && audio_feedback.targeted_animal_entity.is_some() {
            play_audio_cue_requests.write(PlayAudioCue {
                cue: AssetId(tranquilizer_mode_policy.lose_tracking_cue.0),
                emitter: Some(controller_entity),
                selection: controller_entity.to_bits(),
                priority: 192,
                force_looped: false,
            });
        }
        let charge_is_ready =
            tranquilizer_tool.charge_points >= tranquilizer_tool.required_charge_points;
        if charge_is_ready && !audio_feedback.charge_is_ready {
            play_audio_cue_requests.write(PlayAudioCue {
                cue: AssetId(tranquilizer_mode_policy.end_charge_cue.0),
                emitter: Some(controller_entity),
                selection: controller_entity.to_bits(),
                priority: 192,
                force_looped: false,
            });
        }
        audio_feedback.targeted_animal_entity = targeted_animal_entity;
        audio_feedback.charge_is_ready = charge_is_ready;
        if audio_feedback.trigger_is_held && !pointer_input.pressed {
            fire_requests.write(FireTranquilizer {
                controller: controller_entity,
            });
        }
        audio_feedback.trigger_is_held = pointer_input.pressed;
        aim_requests.write(AimTranquilizer {
            controller: controller_entity,
        });
        for action_request in action_requests.read() {
            if action_request.action == GameAction::Cancel {
                stop_audio_cue_requests.write(StopAudioCue {
                    emitter: Some(controller_entity),
                    selection: charge_audio_selection,
                });
                commands.entity(controller_entity).remove::<(
                    TranquilizerControl,
                    TranquilizerTool,
                    crate::plugins::animal_health::types::TranquilizerHud,
                    TranquilizerAudioFeedback,
                )>();
                hide_ui_document_requests.write(HideUiDocument {
                    owner: controller_entity,
                });
            }
        }
    }
}

fn find_animal_ancestor(
    mut entity: Entity,
    parents: &Query<&ChildOf>,
    animals: &Query<
        (&GlobalTransform, Option<&Escaped>, Option<&Rampaging>),
        (With<Animal>, Without<Dead>),
    >,
) -> Option<Entity> {
    loop {
        if animals.contains(entity) {
            return Some(entity);
        }
        entity = parents.get(entity).ok()?.parent();
    }
}
