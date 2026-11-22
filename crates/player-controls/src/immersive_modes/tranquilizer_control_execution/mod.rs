use std::cell::RefCell;

use avian3d::prelude::{SpatialQuery, SpatialQueryFilter};
use bevy::{ecs::entity::EntityHashMap, prelude::*};

use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use openzt2_game_assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use openzt2_interface_state::camera::camera_runtime_state_types::ZooCamera;
use openzt2_interface_state::input::input_types::{ActionRequest, GameAction, MousePointer};
use openzt2_interface_state::ui::input_frame_capture_types::AuthoredModalInputCapture;
use openzt2_interface_state::ui::ui_document_lifecycle_contracts::HideUiDocument;
use openzt2_zoo_state::animal_health::tranquilizer_eligibility_calculation::authored_tranquilizer_allows_animal_state;
use openzt2_zoo_state::animal_health::types::{
    Dead, Escaped, Rampaging, TranquilizeRequest, TranquilizerFireOutcome, TranquilizerFired,
    TranquilizerHud, TranquilizerReticle, TranquilizerTool,
};
use openzt2_zoo_state::animal_lifecycle::types::Animal;
use openzt2_zoo_state::audio::audio_playback_message_types::{PlayAudioCue, StopAudioCue};

use super::animal_care_control_types::{
    AnimalCareTarget, TranquilizerAimSample, TranquilizerAudioFeedback, TranquilizerControl,
    TranquilizerMisfireTimer,
};

pub(super) fn initialize_tranquilizer_tool(
    mut commands: Commands,
    assets: Res<Assets<WorldDefinitionAsset>>,
    definitions: Res<WorldDefinitions>,
    pending: Query<
        (Entity, &AnimalCareTarget),
        (With<TranquilizerControl>, Without<TranquilizerTool>),
    >,
) {
    let Ok((controller, initial_target)) = pending.single() else {
        return;
    };
    let Some(definitions) = definitions.get(&assets) else {
        return;
    };
    let (Some(dose), Some(policy)) = (
        definitions.tranquilizers().next(),
        definitions.tranquilizer_mode(),
    ) else {
        return;
    };
    commands.entity(controller).insert((
        TranquilizerTool {
            target: initial_target.animal_entity,
            tranquilizer: dose.id,
            charge_points: 0.0,
            required_charge_points: policy.required_charge_points,
        },
        TranquilizerAimSample::default(),
        TranquilizerAudioFeedback::default(),
        TranquilizerMisfireTimer::default(),
        TranquilizerHud {
            distance_m: 0.0,
            in_range: false,
            reticle: TranquilizerReticle::NoTarget,
            misfire_visible: false,
        },
    ));
}

type EligibleAnimals<'w, 's> = Query<
    'w,
    's,
    (&'static GlobalTransform, Has<Escaped>, Has<Rampaging>),
    (With<Animal>, Without<Dead>),
>;

pub(super) fn sample_tranquilizer_aim(
    spatial_query: SpatialQuery,
    parents: Query<&ChildOf>,
    animals: EligibleAnimals,
    cameras: Query<(Entity, &GlobalTransform), With<ZooCamera>>,
    assets: Res<Assets<WorldDefinitionAsset>>,
    definitions: Res<WorldDefinitions>,
    modal_input: Res<AuthoredModalInputCapture>,
    mut controller: Query<
        (&TranquilizerTool, &mut TranquilizerAimSample),
        With<TranquilizerControl>,
    >,
) {
    let Ok((tool, mut aim)) = controller.single_mut() else {
        return;
    };
    if modal_input.0.is_some() {
        return;
    }
    let Some(definitions) = definitions.get(&assets) else {
        aim.set_if_neq(TranquilizerAimSample::default());
        return;
    };
    let (Ok((camera, view)), Some(dose), Some(policy)) = (
        cameras.single(),
        definitions.find_tranquilizer(tool.tranquilizer),
        definitions.tranquilizer_mode(),
    ) else {
        aim.set_if_neq(TranquilizerAimSample::default());
        return;
    };
    let owners = RefCell::new(EntityHashMap::<Option<Entity>>::default());
    let origin = view.translation();
    let hit = spatial_query.cast_ray_predicate(
        origin,
        view.forward(),
        f32::MAX,
        false,
        &SpatialQueryFilter::DEFAULT,
        &|collider| {
            owners
                .borrow_mut()
                .entry(collider)
                .or_insert_with(|| {
                    find_animal_ancestor(collider, &parents, &animals).filter(|entity| {
                        animals.get(*entity).is_ok_and(|(_, escaped, rampaging)| {
                            authored_tranquilizer_allows_animal_state(
                                dose.eligible_states,
                                escaped,
                                rampaging,
                            )
                        })
                    })
                })
                .is_some()
        },
    );
    let owners = owners.into_inner();
    let target = hit.and_then(|hit| owners.get(&hit.entity).copied().flatten());
    let distance_m = target
        .and_then(|target| animals.get(target).ok())
        .map_or(0.0, |(pose, ..)| origin.distance(pose.translation()));
    aim.set_if_neq(TranquilizerAimSample {
        camera: Some(camera),
        target,
        distance_m,
        in_range: target.is_some() && distance_m <= policy.range_cm as f32 / 100.0,
    });
}

fn find_animal_ancestor(
    mut entity: Entity,
    parents: &Query<&ChildOf>,
    animals: &EligibleAnimals,
) -> Option<Entity> {
    loop {
        if animals.contains(entity) {
            return Some(entity);
        }
        entity = parents.get(entity).ok()?.parent();
    }
}

pub(super) fn apply_tranquilizer_input(
    time: Res<Time>,
    assets: Res<Assets<WorldDefinitionAsset>>,
    definitions: Res<WorldDefinitions>,
    (pointer, modal_input): (MousePointer, Res<AuthoredModalInputCapture>),
    mut controller: Query<
        (
            Entity,
            &TranquilizerAimSample,
            &mut TranquilizerTool,
            &mut TranquilizerHud,
            &mut TranquilizerAudioFeedback,
            &mut TranquilizerMisfireTimer,
        ),
        With<TranquilizerControl>,
    >,
    mut actions: MessageReader<ActionRequest>,
    mut tranquilize: MessageWriter<TranquilizeRequest>,
    mut fired: MessageWriter<TranquilizerFired>,
    mut play: MessageWriter<PlayAudioCue>,
    mut stop: MessageWriter<StopAudioCue>,
    mut close_hud: MessageWriter<HideUiDocument>,
    mut commands: Commands,
) {
    let Ok((controller, aim, mut tool, mut hud, mut feedback, mut misfire)) =
        controller.single_mut()
    else {
        actions.clear();
        return;
    };
    let loop_selection = controller.to_bits().wrapping_add(1);
    if let Some(timer) = &mut misfire.0 {
        timer.tick(time.delta());
        if timer.is_finished() {
            misfire.0 = None;
        }
    }
    if modal_input.0.is_some() {
        actions.clear();
        if feedback.trigger_is_held {
            stop.write(StopAudioCue {
                emitter: Some(controller),
                selection: loop_selection,
            });
            feedback.trigger_is_held = false;
        }
        if hud.misfire_visible != misfire.0.is_some() {
            hud.misfire_visible = misfire.0.is_some();
        }
        return;
    }
    let cancel = actions
        .read()
        .any(|request| request.action == GameAction::Cancel);
    actions.clear();
    if cancel {
        stop.write(StopAudioCue {
            emitter: Some(controller),
            selection: loop_selection,
        });
        commands.entity(controller).remove::<(
            TranquilizerControl,
            TranquilizerAimSample,
            TranquilizerTool,
            TranquilizerHud,
            TranquilizerAudioFeedback,
            TranquilizerMisfireTimer,
        )>();
        close_hud.write(HideUiDocument { owner: controller });
        return;
    }
    let Some(policy) = definitions
        .get(&assets)
        .and_then(|definitions| definitions.tranquilizer_mode())
    else {
        return;
    };
    let Some(source) = aim.camera else {
        return;
    };
    let trigger = pointer.primary_is_pressed();
    let charging = aim.in_range && trigger;
    let mut next = *tool;
    next.target = aim.target;
    next.required_charge_points = policy.required_charge_points;
    let rate = if charging {
        policy.charge_points_per_second
    } else {
        -policy.charge_decrease_points_per_second
    };
    next.charge_points =
        (next.charge_points + time.delta_secs() * rate).clamp(0.0, next.required_charge_points);
    let mut ready = next.charge_points >= next.required_charge_points;
    let mut cue = |cue, selection, priority, force_looped| {
        play.write(PlayAudioCue {
            cue,
            emitter: Some(controller),
            selection,
            priority,
            force_looped,
        });
    };
    if charging && (aim.target != feedback.targeted_animal_entity || !feedback.trigger_is_held) {
        if feedback.trigger_is_held {
            stop.write(StopAudioCue {
                emitter: Some(controller),
                selection: loop_selection,
            });
        }
        cue(policy.begin_charge_cue, controller.to_bits(), 192, false);
        cue(policy.charge_cue, loop_selection, 191, true);
    } else if !charging && feedback.trigger_is_held {
        stop.write(StopAudioCue {
            emitter: Some(controller),
            selection: loop_selection,
        });
    }
    if aim.target.is_none() && feedback.targeted_animal_entity.is_some() {
        cue(policy.lose_tracking_cue, controller.to_bits(), 192, false);
    }
    if ready && !feedback.charge_is_ready {
        cue(policy.end_charge_cue, controller.to_bits(), 192, false);
    }
    feedback.targeted_animal_entity = aim.target;
    feedback.charge_is_ready = ready;
    if feedback.trigger_is_held && !trigger {
        let outcome = match (ready, aim.target, aim.in_range) {
            (false, _, _) => TranquilizerFireOutcome::Misfire,
            (true, None, _) => TranquilizerFireOutcome::NoTarget,
            (true, Some(_), false) => TranquilizerFireOutcome::OutOfRange,
            (true, Some(animal), true) => {
                // Sedation revalidates the shot against the next simulation step.
                tranquilize.write(TranquilizeRequest {
                    animal,
                    source,
                    tranquilizer: next.tranquilizer,
                });
                TranquilizerFireOutcome::Shot
            }
        };
        fired.write(TranquilizerFired {
            controller,
            tranquilizer: next.tranquilizer,
            outcome,
        });
        next.charge_points = 0.0;
        ready = false;
        if outcome == TranquilizerFireOutcome::Misfire {
            misfire.0 = Some(Timer::from_seconds(
                policy.misfire_duration_seconds,
                TimerMode::Once,
            ));
        }
    }
    feedback.trigger_is_held = trigger;
    tool.set_if_neq(next);
    hud.set_if_neq(TranquilizerHud {
        distance_m: aim.distance_m,
        in_range: aim.in_range,
        reticle: match (aim.target, aim.in_range, ready) {
            (None, _, _) => TranquilizerReticle::NoTarget,
            (Some(_), false, _) => TranquilizerReticle::OutOfRange,
            (Some(_), true, false) => TranquilizerReticle::Charging,
            (Some(_), true, true) => TranquilizerReticle::Ready,
        },
        misfire_visible: misfire.0.is_some(),
    });
}
