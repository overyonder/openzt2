use bevy::prelude::*;

use openzt2_game_assets::application_lifecycle::GamePhase;
use openzt2_interface_state::information::catalogue_types::CatalogueRowEntry;
use openzt2_interface_state::ui::authored_reusable_list_and_table_runtime_types::UiListRow;
use openzt2_interface_state::ui::authored_ui_interaction_enabled_state::UiInteractionEnabled;
use openzt2_world_state::world_spawn::selected_world_identity::SelectedWorldIdentity;
use openzt2_world_state::world_spawn::world_membership_types::WorldRoot;

use super::{
    verification_capture_readiness::VerificationCaptureReadiness,
    verification_catalogue_wheel_diagnostics::{
        observes_remembered_catalogue_journey, CatalogueWheelDiagnostic,
    },
    verification_catalogue_wheel_input::VerificationCatalogueWheelInput,
    verification_game_phase_readiness::{game_phase_order, VerificationGamePhaseReadiness},
    verification_gamepad_input::VerificationGamepadInput,
    verification_input_injection::VerificationInputInjection,
    verification_journey_report::{
        compare_verification_fact, finish_current_verification_journey,
        record_verification_failure, verification_comparison_holds,
        write_current_verification_journey_report,
    },
    verification_journey_state_types::{
        VerificationFactCollectionState, VerificationJourneyReport, VerificationJourneySession,
        VerificationSessionStage,
    },
    verification_journey_types::{
        VerificationCondition, VerificationJourneyAction, VerificationJourneyStep,
        VerificationPointerTarget, VerificationSessionPlace,
    },
    verification_pointer_reach::VerificationPointerReach,
    verification_session_navigation::{
        advance_in_game_dismissal, advance_main_menu_dismissal,
        choose_route_to_current_journey_start, match_known_state, route_steps_to_new_map,
        VerificationSessionObservation,
    },
};

/// Long enough for a map to load on a slow machine. Loading runs on wall time,
/// not frames, since unpaced headless frames can pass much faster.
const PHASE_WAIT_WALL_LIMIT: std::time::Duration = std::time::Duration::from_secs(90);
const NAMED_NODE_WAIT_FRAME_LIMIT: u32 = 600;
const DRAG_MOVEMENT_FRAMES: u32 = 8;
/// Half a second at 60 Hz, plus margin, between presses on the same spot.
const DOUBLE_PRESS_GUARD_FRAMES: u64 = 36;
/// Frames a click follows a node that is still moving, such as a sliding dialog.
const TARGET_REAIM_LIMIT: u32 = 60;
/// Camera metres and degrees that count as no change for `until steady`.
const STEADY_FACT_TOLERANCE: f64 = 1e-4;

pub(crate) type VerificationUiNodeQuery<'w, 's> = Query<
    'w,
    's,
    (
        &'static Name,
        &'static ComputedNode,
        &'static UiGlobalTransform,
        &'static InheritedVisibility,
        Option<&'static UiInteractionEnabled>,
    ),
>;

type VerificationListRowQuery<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static UiListRow,
        &'static InheritedVisibility,
        Option<&'static CatalogueRowEntry>,
    ),
>;

type VerificationNodeTextQuery<'w, 's> = Query<
    'w,
    's,
    (
        &'static Name,
        Option<&'static Text>,
        Option<&'static bevy::text::EditableText>,
        &'static InheritedVisibility,
    ),
>;

pub(crate) enum VerificationStepProgress {
    Running,
    Complete,
    Failed(String),
}

enum VerificationConditionState {
    /// Facts are being collected; the answer arrives on a later frame.
    Pending,
    Holds,
    Broken(String),
}

#[allow(clippy::needless_pass_by_value, clippy::too_many_arguments)]
pub(crate) fn advance_verification_journey_by_one_frame(
    mut commands: Commands,
    mut session: ResMut<VerificationJourneySession>,
    mut report: ResMut<VerificationJourneyReport>,
    readiness: VerificationGamePhaseReadiness,
    capture_readiness: Res<VerificationCaptureReadiness>,
    observation: VerificationSessionObservation,
    nodes: VerificationUiNodeQuery,
    list_rows: VerificationListRowQuery,
    worlds: Query<(&SelectedWorldIdentity, Option<&Name>), With<WorldRoot>>,
    names: Query<&Name>,
    texts: VerificationNodeTextQuery,
    reach: VerificationPointerReach,
    mut input: ParamSet<(VerificationCatalogueWheelInput, VerificationInputInjection)>,
    mut wheel_diagnostic: Option<ResMut<CatalogueWheelDiagnostic>>,
    mut gamepad_input: VerificationGamepadInput,
    mut exit: MessageWriter<AppExit>,
) {
    if session.finished {
        return;
    }
    let phase = readiness.current_phase();
    if phase == GamePhase::Loading && session.observed_phase != GamePhase::Loading {
        session.world_loads_started += 1;
    }
    session.observed_phase = phase;
    session.frames_run += 1;
    if let Some(diagnostic) = wheel_diagnostic.as_deref_mut() {
        diagnostic.retire_changed_step(&session, input.p0().current_world());
    }
    let Some(target) = session
        .capture_target
        .as_ref()
        .and_then(|target| target.normalize(Some(input.p1().primary_window_entity())))
    else {
        return;
    };
    if session.stage == VerificationSessionStage::ChoosingRoute {
        match choose_route_to_current_journey_start(&mut session, phase) {
            Ok((stage, steps)) => begin_session_stage(&mut session, stage, steps),
            Err(message) => fail_navigation(&mut session, &observation, &message),
        }
    }
    let Some(step) = session.steps.get(session.step_index).cloned() else {
        complete_session_stage(&mut session, &mut report, &mut exit);
        input
            .p1()
            .resubmit_stationary_pointer(&target, session.pointer_position);
        return;
    };
    let frame = session.step_frame;
    // Failures from invariant checks or screenshot saving arrive between frames.
    // Finish an input step that already pressed before abandoning its release.
    let finishing_pressed_input = match &step.action {
        VerificationJourneyAction::Click(_) | VerificationJourneyAction::Drag { .. } => session
            .step_resolved_frame
            .is_some_and(|resolved| frame > resolved + 2),
        VerificationJourneyAction::Key { control: true, .. } => frame % 4 != 0,
        VerificationJourneyAction::Key { control: false, .. }
        | VerificationJourneyAction::TypeText(_)
        | VerificationJourneyAction::GamepadButton(_) => frame % 2 == 1,
        _ => false,
    };
    if session.stage == VerificationSessionStage::Journey
        && !report.failures.is_empty()
        && !finishing_pressed_input
    {
        if let VerificationJourneyAction::HoldKeyUntil { key_code, .. } = &step.action {
            input.p1().press_or_release_key(*key_code, false);
        }
        begin_failed_journey_capture(&mut session, &mut report, &observation);
        input
            .p1()
            .resubmit_stationary_pointer(&target, session.pointer_position);
        return;
    }
    let mut pointer_input_written = false;
    // Bound the whole game's real time and a journey's frame rate independently of
    // simulation frames. Do this before issuing another input, and finish any
    // ordinary click/key release already in flight.
    let budget_spent =
        !session.budget_exhausted && std::time::Instant::now() >= session.run_deadline;
    let slow_frames = frame_rate_below_minimum(&mut session, phase);
    if (budget_spent || slow_frames.is_some()) && !finishing_pressed_input {
        input
            .p1()
            .press_or_release_primary_button(&target, session.pointer_position, false);
        if let VerificationJourneyAction::HoldKeyUntil { key_code, .. } = &step.action {
            input.p1().press_or_release_key(*key_code, false);
        }
        if let super::verification_journey_state_types::VerificationDismissalWait::ReleaseKey {
            key_code,
            ..
        } = session.step_dismissal.wait
        {
            input.p1().press_or_release_key(key_code, false);
        }
        let message = if budget_spent {
            session.budget_exhausted = true;
            format!(
                "the run exceeded its {}-second budget after {frame} frames of {:?}",
                session.budget.as_secs(),
                step.action
            )
        } else {
            format!(
                "{} after {frame} frames of {:?}",
                slow_frames.unwrap_or_default(),
                step.action
            )
        };
        if budget_spent && session.stage != VerificationSessionStage::Journey {
            fail_navigation(&mut session, &observation, &message);
        } else {
            fail_session_step(
                &mut session,
                &mut report,
                &observation,
                phase,
                &step,
                message,
            );
        }
        return;
    }
    let progress = match &step.action {
        VerificationJourneyAction::WaitForPhase(phase) => {
            if readiness.phase_is_ready_for_input(*phase) {
                VerificationStepProgress::Complete
                // Only a return to the main menu moves backwards through the phases.
            } else if *phase != GamePhase::MainMenu
                && game_phase_order(readiness.current_phase()) > game_phase_order(*phase)
            {
                VerificationStepProgress::Failed(format!(
                    "game reached {:?} before {phase:?} was ready",
                    readiness.current_phase()
                ))
            } else if session.step_started_at.elapsed() >= PHASE_WAIT_WALL_LIMIT {
                VerificationStepProgress::Failed(format!(
                    "{phase:?} was not ready after {frame} frames (current phase {:?})",
                    readiness.current_phase()
                ))
            } else {
                VerificationStepProgress::Running
            }
        }
        VerificationJourneyAction::WaitFrames(frames) => {
            if frame >= *frames {
                VerificationStepProgress::Complete
            } else {
                VerificationStepProgress::Running
            }
        }
        VerificationJourneyAction::WaitUntil {
            condition,
            frame_limit,
        } => match evaluate_verification_condition(
            &mut session,
            &report,
            condition,
            &nodes,
            &list_rows,
            &names,
            &texts,
            &worlds,
        ) {
            VerificationConditionState::Holds => VerificationStepProgress::Complete,
            VerificationConditionState::Broken(message) if frame >= *frame_limit => {
                log_visible_named_nodes_for_failed_visibility(condition, &nodes);
                VerificationStepProgress::Failed(format!(
                    "not true after {frame} frames: {message}"
                ))
            }
            VerificationConditionState::Pending | VerificationConditionState::Broken(_) => {
                VerificationStepProgress::Running
            }
        },
        VerificationJourneyAction::WaitUntilSteady {
            fact,
            settled_value,
            steady_frames,
            frame_limit,
        } => {
            if !collect_facts_when_ready(&mut session) {
                VerificationStepProgress::Running
            } else if let Some(value) = report.current_facts.get(fact).copied() {
                match session.step_steady_value {
                    Some((last, since))
                        if (value - last).abs() <= STEADY_FACT_TOLERANCE
                            && settled_value.is_none_or(|settled| {
                                (value - settled).abs() <= STEADY_FACT_TOLERANCE
                            }) =>
                    {
                        if frame - since >= *steady_frames {
                            VerificationStepProgress::Complete
                        } else if frame >= *frame_limit {
                            VerificationStepProgress::Failed(format!(
                                "{fact} was still changing after {frame} frames"
                            ))
                        } else {
                            VerificationStepProgress::Running
                        }
                    }
                    _ if frame >= *frame_limit => VerificationStepProgress::Failed(format!(
                        "{fact} was still changing after {frame} frames"
                    )),
                    _ => {
                        session.step_steady_value = Some((value, frame));
                        VerificationStepProgress::Running
                    }
                }
            } else {
                VerificationStepProgress::Failed(format!("unknown fact `{fact}`"))
            }
        }
        VerificationJourneyAction::MovePointer(pointer_target) => {
            let position = {
                let catalogue = input.p0();
                resolve_step_pointer_position(
                    &mut session,
                    pointer_target,
                    &catalogue,
                    &nodes,
                    &list_rows,
                    &names,
                    &reach,
                    frame,
                )
            };
            match position {
                Err(message) => VerificationStepProgress::Failed(message),
                Ok(None) => VerificationStepProgress::Running,
                Ok(Some(position)) => {
                    input
                        .p1()
                        .move_pointer(&target, session.pointer_position, position);
                    session.pointer_position = position;
                    pointer_input_written = true;
                    VerificationStepProgress::Complete
                }
            }
        }
        VerificationJourneyAction::Click(pointer_target) => {
            let position = {
                let catalogue = input.p0();
                resolve_step_pointer_position(
                    &mut session,
                    pointer_target,
                    &catalogue,
                    &nodes,
                    &list_rows,
                    &names,
                    &reach,
                    frame,
                )
            };
            match position {
                Err(message) => VerificationStepProgress::Failed(message),
                Ok(None) => VerificationStepProgress::Running,
                Ok(Some(position)) => {
                    // Picking needs a frame to hover the target before the press.
                    let resolved_frame = *session.step_resolved_frame.get_or_insert(frame);
                    match frame - resolved_frame {
                        0 => {
                            input
                                .p1()
                                .move_pointer(&target, session.pointer_position, position);
                            session.pointer_position = position;
                            pointer_input_written = true;
                            VerificationStepProgress::Running
                        }
                        1 => {
                            aim_again_if_the_target_moved_or_is_covered(
                                &mut session,
                                &nodes,
                                &reach,
                                position,
                                frame,
                            );
                            VerificationStepProgress::Running
                        }
                        2 if session
                            .step_target
                            .is_some_and(|node| !reach.pointer_reaches(node)) =>
                        {
                            if frame >= NAMED_NODE_WAIT_FRAME_LIMIT {
                                VerificationStepProgress::Failed(format!(
                                    "the pointer cannot reach {pointer_target:?} after {frame} frames"
                                ))
                            } else {
                                // Re-aim after layout or modal changes before pressing.
                                session.step_resolved_frame = Some(frame + 1);
                                VerificationStepProgress::Running
                            }
                        }
                        2 if pressed_too_soon_after_the_last_press(&session, position) => {
                            // Hold at the press until the double-press window has passed.
                            session.step_resolved_frame = Some(resolved_frame + 1);
                            VerificationStepProgress::Running
                        }
                        2 => {
                            input
                                .p1()
                                .press_or_release_primary_button(&target, position, true);
                            session.last_primary_press = Some((position, session.frames_run));
                            pointer_input_written = true;
                            VerificationStepProgress::Running
                        }
                        4 => {
                            input
                                .p1()
                                .press_or_release_primary_button(&target, position, false);
                            pointer_input_written = true;
                            VerificationStepProgress::Running
                        }
                        6.. => VerificationStepProgress::Complete,
                        _ => VerificationStepProgress::Running,
                    }
                }
            }
        }
        VerificationJourneyAction::Drag { from, to } => {
            let position = {
                let catalogue = input.p0();
                resolve_step_pointer_position(
                    &mut session,
                    from,
                    &catalogue,
                    &nodes,
                    &list_rows,
                    &names,
                    &reach,
                    frame,
                )
            };
            match position {
                Err(message) => VerificationStepProgress::Failed(message),
                Ok(None) => VerificationStepProgress::Running,
                Ok(Some(start)) => {
                    let resolved_frame = *session.step_resolved_frame.get_or_insert(frame);
                    let elapsed = frame - resolved_frame;
                    let release_frame = 4 + DRAG_MOVEMENT_FRAMES;
                    if elapsed == 0 {
                        input
                            .p1()
                            .move_pointer(&target, session.pointer_position, start);
                        session.pointer_position = start;
                        pointer_input_written = true;
                    } else if elapsed == 2 {
                        input
                            .p1()
                            .press_or_release_primary_button(&target, start, true);
                        pointer_input_written = true;
                    } else if (3..release_frame).contains(&elapsed) {
                        #[allow(clippy::cast_precision_loss)]
                        let fraction = (elapsed - 2) as f32 / DRAG_MOVEMENT_FRAMES as f32;
                        let position = start.lerp(*to, fraction.min(1.0));
                        input
                            .p1()
                            .move_pointer(&target, session.pointer_position, position);
                        session.pointer_position = position;
                        pointer_input_written = true;
                    } else if elapsed == release_frame {
                        input
                            .p1()
                            .move_pointer(&target, session.pointer_position, *to);
                        session.pointer_position = *to;
                        input
                            .p1()
                            .press_or_release_primary_button(&target, *to, false);
                        pointer_input_written = true;
                    }
                    if elapsed > release_frame + 1 {
                        VerificationStepProgress::Complete
                    } else {
                        VerificationStepProgress::Running
                    }
                }
            }
        }
        VerificationJourneyAction::Key {
            key_code,
            control,
            repeat,
        } => {
            // Keep Control held through key release: authored shortcuts may run there.
            let frames_per_press = if *control { 4 } else { 2 };
            if frame < repeat * frames_per_press {
                let offset = frame % frames_per_press;
                if *control && (offset == 0 || offset == 3) {
                    input
                        .p1()
                        .press_or_release_key(KeyCode::ControlLeft, offset == 0);
                } else {
                    input
                        .p1()
                        .press_or_release_key(*key_code, offset == u32::from(*control));
                }
                VerificationStepProgress::Running
            } else if frame < repeat * frames_per_press + 2 {
                VerificationStepProgress::Running
            } else {
                VerificationStepProgress::Complete
            }
        }
        VerificationJourneyAction::HoldKeyUntil {
            key_code,
            condition,
            frame_limit,
        } => {
            let state = evaluate_verification_condition(
                &mut session,
                &report,
                condition,
                &nodes,
                &list_rows,
                &names,
                &texts,
                &worlds,
            );
            if matches!(state, VerificationConditionState::Holds) {
                input.p1().press_or_release_key(*key_code, false);
                VerificationStepProgress::Complete
            } else if frame >= *frame_limit {
                input.p1().press_or_release_key(*key_code, false);
                VerificationStepProgress::Failed(format!(
                    "held {key_code:?} for {frame} frames without reaching {condition:?}"
                ))
            } else {
                if frame == 0 {
                    input.p1().press_or_release_key(*key_code, true);
                }
                VerificationStepProgress::Running
            }
        }
        VerificationJourneyAction::TypeText(text) => {
            // One character per two frames: press, then release.
            let characters = text.chars().collect::<Vec<_>>();
            let character_index = (frame / 2) as usize;
            if let Some(character) = characters.get(character_index) {
                input
                    .p1()
                    .press_or_release_text_character(*character, frame % 2 == 0);
                VerificationStepProgress::Running
            } else {
                VerificationStepProgress::Complete
            }
        }
        VerificationJourneyAction::ConnectGamepad => {
            if frame == 0 {
                match gamepad_input.connect() {
                    Ok(()) => VerificationStepProgress::Running,
                    Err(message) => VerificationStepProgress::Failed(message),
                }
            } else if gamepad_input.is_connected() {
                VerificationStepProgress::Complete
            } else if frame >= 60 {
                VerificationStepProgress::Failed("Bevy did not connect the gamepad".to_owned())
            } else {
                VerificationStepProgress::Running
            }
        }
        VerificationJourneyAction::GamepadButton(button) => {
            if frame < 2 {
                match gamepad_input.press_or_release_button(*button, frame == 0) {
                    Ok(()) => VerificationStepProgress::Running,
                    Err(message) => VerificationStepProgress::Failed(message),
                }
            } else if frame < 4 {
                VerificationStepProgress::Running
            } else {
                VerificationStepProgress::Complete
            }
        }
        VerificationJourneyAction::DisconnectGamepad => {
            if gamepad_input.disconnect() {
                VerificationStepProgress::Complete
            } else if frame >= 60 {
                VerificationStepProgress::Failed("Bevy did not disconnect the gamepad".to_owned())
            } else {
                VerificationStepProgress::Running
            }
        }
        VerificationJourneyAction::Wheel { lines, repeat } => {
            // One wheel event every other frame, as separate notches arrive natively.
            if frame < repeat * 2 {
                if frame % 2 == 0 {
                    input.p1().scroll_wheel_lines(*lines);
                }
                VerificationStepProgress::Running
            } else {
                VerificationStepProgress::Complete
            }
        }
        VerificationJourneyAction::WheelToCatalogue {
            definition_key,
            definition,
            list_name,
        } => {
            if frame == 0 || frame == 2 {
                // Finish reading list geometry and the shared window before
                // input injection mutates the cursor window and marker Node.
                let plan = {
                    let catalogue_wheel = input.p0();
                    let scope = list_name
                        .as_deref()
                        .map(|name| catalogue_wheel.resolve_active_hud_list(name))
                        .transpose();
                    scope
                        .and_then(|list| catalogue_wheel.plan_row_wheel_input(*definition, list))
                        .map(|plan| {
                            if wheel_diagnostic.is_some()
                                && observes_remembered_catalogue_journey(&session)
                            {
                                info!(target: "openzt2_verification", frame = session.frames_run,
                                    step = session.step_index, ?definition, row = ?plan.row,
                                    list = ?plan.list, planned_pointer = ?plan.point, pixels = plan.pixels,
                                    pointer_context_before_move = ?session.pointer_position,
                                    "catalogue wheel planned input; pointer context and latest layout precede PreUpdate picking");
                                catalogue_wheel.log_catalogue_wheel_row_boundary(
                                    plan.row,
                                    *definition,
                                    plan.list,
                                    &reach,
                                    if frame == 0 {
                                        "planning"
                                    } else {
                                        "before-emission"
                                    },
                                    session.frames_run,
                                    Some(plan.pixels),
                                );
                            }
                            (plan, catalogue_wheel.pointer_wheel_reaches_list(plan.list))
                        })
                };
                match plan {
                    Err(detail) => VerificationStepProgress::Failed(format!(
                        "cannot wheel to {definition_key}: {detail}"
                    )),
                    Ok((plan, wheel_reaches_list)) => {
                        let list = plan.list;
                        let point = plan.point;
                        if frame == 0 {
                            if list_name.is_some() {
                                session.step_target = Some(list);
                            }
                            input
                                .p1()
                                .move_pointer(&target, session.pointer_position, point);
                            session.pointer_position = point;
                            pointer_input_written = true;
                            VerificationStepProgress::Running
                        } else if list_name.is_some() && session.step_target != Some(list) {
                            VerificationStepProgress::Failed(
                                "the scoped catalogue list changed before its wheel event"
                                    .to_owned(),
                            )
                        } else if !wheel_reaches_list {
                            VerificationStepProgress::Failed(format!(
                                "the catalogue viewport for {definition_key} is blocked by the current wheel hit owner"
                            ))
                        } else {
                            if wheel_diagnostic.is_some()
                                && observes_remembered_catalogue_journey(&session)
                            {
                                let world = input.p0().current_world();
                                if let Some(diagnostic) = wheel_diagnostic.as_deref_mut() {
                                    diagnostic.observe_emission(&session, *definition, plan, world);
                                }
                                info!(target: "openzt2_verification", frame = session.frames_run,
                                    step = session.step_index, list = ?plan.list, row = ?plan.row,
                                    unit = "Pixel", x = 0.0, y = plan.pixels,
                                    window = ?input.p1().primary_window_entity(),
                                    "catalogue wheel emitting native MouseWheel");
                            }
                            input.p1().scroll_wheel_pixels(plan.pixels);
                            VerificationStepProgress::Running
                        }
                    }
                }
            } else if frame >= 4 {
                VerificationStepProgress::Complete
            } else {
                VerificationStepProgress::Running
            }
        }
        VerificationJourneyAction::Snapshot(name) => {
            if collect_facts_when_ready(&mut session) {
                let facts = report.current_facts.clone();
                report.snapshots.insert(name.clone(), facts);
                VerificationStepProgress::Complete
            } else {
                VerificationStepProgress::Running
            }
        }
        VerificationJourneyAction::Capture(name) => {
            super::verification_capture_execution::advance_verification_capture(
                &mut commands,
                &mut session,
                &mut report,
                &capture_readiness,
                name,
                &step.source_location,
                frame,
            )
        }
        VerificationJourneyAction::MeasureFrameTime(frames) => {
            session.measuring_frame_work = frame < *frames;
            if frame > *frames {
                report.frame_time_facts = summarize_frame_time_milliseconds(std::mem::take(
                    &mut session.frame_time_milliseconds,
                ));
                VerificationStepProgress::Complete
            } else {
                VerificationStepProgress::Running
            }
        }
        VerificationJourneyAction::Expect(condition) => {
            match evaluate_verification_condition(
                &mut session,
                &report,
                condition,
                &nodes,
                &list_rows,
                &names,
                &texts,
                &worlds,
            ) {
                VerificationConditionState::Pending => VerificationStepProgress::Running,
                VerificationConditionState::Holds => VerificationStepProgress::Complete,
                VerificationConditionState::Broken(message) => {
                    log_visible_named_nodes_for_failed_visibility(condition, &nodes);
                    VerificationStepProgress::Failed(message)
                }
            }
        }
        VerificationJourneyAction::ReturnInGameToOptions => {
            pointer_input_written = true;
            advance_in_game_dismissal(
                &mut session,
                &observation,
                &nodes,
                &reach,
                &mut input.p1(),
                &target,
                frame,
                None,
            )
        }
        VerificationJourneyAction::ReturnInGameToKnownState(place) => {
            pointer_input_written = true;
            advance_in_game_dismissal(
                &mut session,
                &observation,
                &nodes,
                &reach,
                &mut input.p1(),
                &target,
                frame,
                Some(*place),
            )
        }
        VerificationJourneyAction::ReturnToMainMenuScreen => {
            pointer_input_written = true;
            advance_main_menu_dismissal(
                &mut session,
                &observation,
                &nodes,
                &reach,
                &mut input.p1(),
                &target,
                frame,
            )
        }
        VerificationJourneyAction::MatchKnownState(place) => {
            if collect_facts_when_ready(&mut session) {
                match match_known_state(
                    &mut session,
                    *place,
                    observation.significant_visible_roles(),
                    &report.current_facts,
                ) {
                    Ok(()) => VerificationStepProgress::Complete,
                    Err(message) => VerificationStepProgress::Failed(message),
                }
            } else {
                VerificationStepProgress::Running
            }
        }
    };
    if !pointer_input_written {
        input
            .p1()
            .resubmit_stationary_pointer(&target, session.pointer_position);
    }
    match progress {
        VerificationStepProgress::Running => session.step_frame += 1,
        VerificationStepProgress::Complete => {
            debug!(target: "openzt2_verification", location = %step.source_location, "journey step complete");
            if session.stage == VerificationSessionStage::Journey && !report.failures.is_empty() {
                begin_failed_journey_capture(&mut session, &mut report, &observation);
            } else {
                let mut next_step = session.step_index + 1;
                // Consecutive numeric assertions describe one observation. Do not
                // advance the simulation and recollect every domain for each field.
                if matches!(
                    step.action,
                    VerificationJourneyAction::Expect(VerificationCondition::Fact(_))
                ) {
                    while let Some(next) = session.steps.get(next_step).cloned() {
                        let VerificationJourneyAction::Expect(VerificationCondition::Fact(
                            expectation,
                        )) = &next.action
                        else {
                            break;
                        };
                        if let Err(message) = compare_verification_fact(&report, expectation) {
                            begin_step(&mut session, next_step);
                            fail_session_step(
                                &mut session,
                                &mut report,
                                &observation,
                                phase,
                                &next,
                                message,
                            );
                            return;
                        }
                        debug!(target: "openzt2_verification", location = %next.source_location, "journey step complete");
                        next_step += 1;
                    }
                }
                begin_step(&mut session, next_step);
            }
        }
        VerificationStepProgress::Failed(message) => {
            if wheel_diagnostic.is_some() && observes_remembered_catalogue_journey(&session) {
                if let VerificationJourneyAction::Click(
                    VerificationPointerTarget::CatalogueEntry {
                        definition,
                        list_name,
                        ..
                    },
                ) = &step.action
                {
                    input.p0().log_catalogue_selector_failure(
                        *definition,
                        list_name.as_deref(),
                        &reach,
                        session.frames_run,
                    );
                }
            }
            fail_session_step(
                &mut session,
                &mut report,
                &observation,
                phase,
                &step,
                message,
            );
        }
    }
}

/// A failed journey step ends that journey; a failed reset tries the main menu;
/// a failed route ends the game so the runner can continue in a fresh one.
fn fail_session_step(
    session: &mut VerificationJourneySession,
    report: &mut VerificationJourneyReport,
    observation: &VerificationSessionObservation,
    phase: GamePhase,
    step: &VerificationJourneyStep,
    message: String,
) {
    match session.stage {
        VerificationSessionStage::Journey => {
            record_verification_failure(report, &step.source_location, message);
            begin_failed_journey_capture(session, report, observation);
        }
        VerificationSessionStage::FinishAfterJourneyFailure => {
            // Do not retry a failed evidence capture or lose the original failure.
            record_verification_failure(report, &step.source_location, message);
            begin_session_stage(session, session.stage, Vec::new());
        }
        VerificationSessionStage::ResetWithinMap => {
            // Expected when a journey changed the world or the view; load the map again.
            info!(target: "openzt2_verification", location = %step.source_location,
                detail = %message, "verification in-map reset fell back to the main menu");
            let map = session.journeys[session.journey_index].start.place;
            let steps = route_steps_to_new_map(session, phase, map);
            begin_session_stage(
                session,
                VerificationSessionStage::RouteThroughMainMenu,
                steps,
            );
        }
        VerificationSessionStage::ChoosingRoute
        | VerificationSessionStage::RouteThroughMainMenu => {
            fail_navigation(
                session,
                observation,
                &format!("{}: {message}", step.source_location),
            );
        }
        // The evidence capture itself failed; exit without it.
        VerificationSessionStage::ExitAfterNavigationFailure => {
            begin_session_stage(session, session.stage, Vec::new());
        }
    }
}

fn begin_failed_journey_capture(
    session: &mut VerificationJourneySession,
    report: &mut VerificationJourneyReport,
    observation: &VerificationSessionObservation,
) {
    let location = report
        .failures
        .last()
        .map_or_else(|| "session".to_owned(), |failure| failure.location.clone());
    info!(target: "openzt2_verification", journey = %session.journeys[session.journey_index].name,
        location = %location, phase = ?session.observed_phase,
        visible_roles = ?observation.visible_roles(), step_frame = session.step_frame,
        "ending failed verification journey with evidence capture");
    // Preserve the facts used by the assertion before capture collects newer ones.
    report
        .snapshots
        .insert("failure-at-step".to_owned(), report.current_facts.clone());
    // Keep the known failure on disk even if rendering or readback stops the game.
    write_current_verification_journey_report(session, report);
    session.measuring_frame_work = false;
    session.frame_time_milliseconds.clear();
    begin_session_stage(
        session,
        VerificationSessionStage::FinishAfterJourneyFailure,
        vec![VerificationJourneyStep {
            source_location: location,
            action: VerificationJourneyAction::Capture("journey-failure".to_owned()),
        }],
    );
}

/// Called the frame after the pointer moved onto a node. A node still moving
/// is aimed at again, and a node a press would not reach, such as a hidden copy
/// of a dialog button, gives way to the next node with its name.
fn aim_again_if_the_target_moved_or_is_covered(
    session: &mut VerificationJourneySession,
    nodes: &VerificationUiNodeQuery,
    reach: &VerificationPointerReach,
    position: Vec2,
    frame: u32,
) {
    let Some(target) = session.step_target else {
        return;
    };
    let current = nodes
        .get(target)
        .ok()
        .map(|(_, _, transform, _, _)| transform.translation);
    if let Some(current) = current {
        if current.distance(position) > 0.5 && session.step_target_reaims < TARGET_REAIM_LIMIT {
            session.step_target_reaims += 1;
            session.step_pointer_position = Some(current);
            session.step_resolved_frame = Some(frame + 1);
            return;
        }
    }
    if reach.pointer_reaches(target) || session.step_alternative_targets.is_empty() {
        return;
    }
    let next = session.step_alternative_targets.remove(0);
    if let Ok((name, _, transform, _, _)) = nodes.get(next) {
        info!(target: "openzt2_verification", name = name.as_str(),
            "a press would not reach the named node; trying the next node with its name");
        session.step_target = Some(next);
        session.step_pointer_position = Some(transform.translation);
        session.step_resolved_frame = Some(frame + 1);
    }
}

/// A second press on the same control within half a second of simulated time is a
/// double press (`authored_ui_pointer_activation`). Journeys ask for single clicks.
fn pressed_too_soon_after_the_last_press(
    session: &VerificationJourneySession,
    position: Vec2,
) -> bool {
    session
        .last_primary_press
        .is_some_and(|(last_position, last_frame)| {
            last_position.distance(position) < 2.0
                && session.frames_run - last_frame < DOUBLE_PRESS_GUARD_FRAMES
        })
}

fn begin_step(session: &mut VerificationJourneySession, step_index: usize) {
    session.step_index = step_index;
    session.step_frame = 0;
    session.step_started_at = std::time::Instant::now();
    session.step_pointer_position = None;
    session.step_resolved_frame = None;
    session.step_target = None;
    session.step_alternative_targets.clear();
    session.step_target_reaims = 0;
    session.step_dismissal = default();
    session.step_steady_value = None;
    session.step_capture = default();
    session.fact_collection = VerificationFactCollectionState::Idle;
}

fn begin_session_stage(
    session: &mut VerificationJourneySession,
    stage: VerificationSessionStage,
    steps: Vec<VerificationJourneyStep>,
) {
    session.stage = stage;
    session.steps = steps.into_boxed_slice();
    begin_step(session, 0);
}

/// Moves to the next stage once the current stage's steps have all run.
fn complete_session_stage(
    session: &mut VerificationJourneySession,
    report: &mut VerificationJourneyReport,
    exit: &mut MessageWriter<AppExit>,
) {
    match session.stage {
        VerificationSessionStage::ChoosingRoute => {}
        VerificationSessionStage::ResetWithinMap
        | VerificationSessionStage::RouteThroughMainMenu => {
            let journey = &session.journeys[session.journey_index];
            let place = journey.start.place;
            let mut steps = Vec::with_capacity(journey.steps.len() + 1);
            if matches!(
                place,
                VerificationSessionPlace::FreeformMap | VerificationSessionPlace::ChallengeMap
            ) {
                // Journeys compare the camera and world with the map as they found it.
                steps.push(VerificationJourneyStep {
                    source_location: "session".to_owned(),
                    action: VerificationJourneyAction::Snapshot("entered-map".to_owned()),
                });
            }
            steps.extend_from_slice(&journey.steps);
            info!(target: "openzt2_verification", journey = %journey.name, ?place,
                "verification journey started");
            session.journey_started_at = Some((session.frames_run, std::time::Instant::now()));
            session.place = (place != VerificationSessionPlace::Boot)
                .then_some((place, session.world_loads_started));
            begin_session_stage(session, VerificationSessionStage::Journey, steps);
        }
        VerificationSessionStage::Journey | VerificationSessionStage::FinishAfterJourneyFailure => {
            finish_current_verification_journey(session, report);
            session.journey_index += 1;
            if session.journey_index == session.journeys.len() || session.budget_exhausted {
                exit_verification_session(session, exit);
            } else {
                begin_session_stage(session, VerificationSessionStage::ChoosingRoute, Vec::new());
            }
        }
        VerificationSessionStage::ExitAfterNavigationFailure => {
            exit_verification_session(session, exit);
        }
    }
}

/// Records where navigation stopped and captures the screen as evidence; the
/// stage then exits so the runner continues the remaining journeys in a fresh game.
fn fail_navigation(
    session: &mut VerificationJourneySession,
    observation: &VerificationSessionObservation,
    message: &str,
) {
    let journey = &session.journeys[session.journey_index];
    error!(target: "openzt2_verification", journey = %journey.name, start = ?journey.start.place,
        phase = ?session.observed_phase, visible_roles = ?observation.visible_roles(),
        detail = %message, "verification navigation failure");
    begin_session_stage(
        session,
        VerificationSessionStage::ExitAfterNavigationFailure,
        vec![VerificationJourneyStep {
            source_location: "session".to_owned(),
            action: VerificationJourneyAction::Capture("navigation-failure".to_owned()),
        }],
    );
}

/// Frames per second below which a journey is failing rather than merely slow.
const MINIMUM_JOURNEY_FRAME_RATE: f64 = 10.0;
const FRAME_RATE_WINDOW: std::time::Duration = std::time::Duration::from_secs(10);

/// Measures a journey's own steps in whole windows; world loading restarts it.
fn frame_rate_below_minimum(
    session: &mut VerificationJourneySession,
    phase: GamePhase,
) -> Option<String> {
    if session.stage != VerificationSessionStage::Journey || phase == GamePhase::Loading {
        session.frame_rate_window = None;
        return None;
    }
    let now = std::time::Instant::now();
    let (start_frame, started_at) = *session
        .frame_rate_window
        .get_or_insert((session.frames_run, now));
    let elapsed = now - started_at;
    if elapsed < FRAME_RATE_WINDOW {
        return None;
    }
    session.frame_rate_window = Some((session.frames_run, now));
    let rate = (session.frames_run - start_frame) as f64 / elapsed.as_secs_f64();
    (rate < MINIMUM_JOURNEY_FRAME_RATE).then(|| {
        format!(
            "the frame rate fell to {rate:.1} per second over {:.0} seconds, below the \
             {MINIMUM_JOURNEY_FRAME_RATE} minimum",
            elapsed.as_secs_f64()
        )
    })
}

fn exit_verification_session(
    session: &mut VerificationJourneySession,
    exit: &mut MessageWriter<AppExit>,
) {
    session.finished = true;
    let completed = session.journey_index == session.journeys.len();
    exit.write(if completed && !session.any_journey_failed {
        AppExit::Success
    } else {
        AppExit::error()
    });
}

pub(crate) fn mark_verification_frame_work_start(mut session: ResMut<VerificationJourneySession>) {
    session.frame_work_started = Some(std::time::Instant::now());
}

/// Frame work excludes any time between frames, so it shows headroom as well as overruns.
pub(crate) fn record_verification_frame_work_time(mut session: ResMut<VerificationJourneySession>) {
    if session.measuring_frame_work {
        if let Some(started) = session.frame_work_started {
            let milliseconds = started.elapsed().as_secs_f64() * 1000.0;
            session.frame_time_milliseconds.push(milliseconds);
        }
    }
}

/// Requests facts on the first call and returns true once the collector has filled them.
pub(super) fn collect_facts_when_ready(session: &mut VerificationJourneySession) -> bool {
    match session.fact_collection {
        VerificationFactCollectionState::Idle => {
            session.fact_collection = VerificationFactCollectionState::Requested;
            false
        }
        VerificationFactCollectionState::Requested => false,
        VerificationFactCollectionState::Collected => {
            session.fact_collection = VerificationFactCollectionState::Idle;
            true
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn evaluate_verification_condition(
    session: &mut VerificationJourneySession,
    report: &VerificationJourneyReport,
    condition: &VerificationCondition,
    nodes: &VerificationUiNodeQuery,
    list_rows: &VerificationListRowQuery,
    names: &Query<&Name>,
    texts: &VerificationNodeTextQuery,
    worlds: &Query<(&SelectedWorldIdentity, Option<&Name>), With<WorldRoot>>,
) -> VerificationConditionState {
    let result = match condition {
        VerificationCondition::Fact(expectation) => {
            if !collect_facts_when_ready(session) {
                return VerificationConditionState::Pending;
            }
            compare_verification_fact(report, expectation)
        }
        VerificationCondition::ListRowCount {
            list_name,
            comparison,
            expected,
        } => {
            let visible_rows = list_rows
                .iter()
                .filter(|(_, row, visibility, _)| {
                    visibility.get()
                        && names
                            .get(row.list)
                            .is_ok_and(|name| name.as_str() == list_name)
                })
                .count();
            let actual = f64::from(u32::try_from(visible_rows).unwrap_or(u32::MAX));
            if verification_comparison_holds(*comparison, actual, *expected) {
                Ok(())
            } else {
                Err(format!(
                    "expected {comparison:?} {expected} visible rows in \"{list_name}\", found {actual}"
                ))
            }
        }
        VerificationCondition::NodeText {
            node_name,
            expected,
        } => {
            let shown_texts = texts
                .iter()
                .filter(|(name, _, _, visibility)| name.as_str() == node_name && visibility.get())
                .map(|(_, text, editable, _)| {
                    editable.map_or_else(
                        || text.map_or_else(String::new, |text| text.0.clone()),
                        |editable| editable.value().to_string(),
                    )
                })
                .collect::<Vec<_>>();
            if shown_texts.iter().any(|shown| shown == expected) {
                Ok(())
            } else {
                Err(format!(
                    "expected \"{node_name}\" to show {expected:?}, found {shown_texts:?}"
                ))
            }
        }
        VerificationCondition::WorldZooName { expected } => {
            if worlds
                .single()
                .is_ok_and(|(_, name)| name.is_some_and(|name| name.as_str() == expected))
            {
                Ok(())
            } else {
                Err(format!(
                    "expected live zoo name {expected:?}, found {:?}",
                    worlds
                        .iter()
                        .map(|(_, name)| name.map(Name::as_str))
                        .collect::<Vec<_>>()
                ))
            }
        }
        VerificationCondition::CampaignWorld { scenario, map } => {
            let expected_scenario = openzt2_game_data::AssetId::from_key(scenario);
            let expected_map = openzt2_game_data::AssetId::from_key(map);
            if worlds.iter().any(|(world, _)| {
                world.mode == openzt2_world_state::game_session_types::WorldSessionMode::Campaign
                    && world.requested == expected_scenario
                    && world.map == expected_map
            }) {
                Ok(())
            } else {
                Err(format!(
                    "expected campaign scenario {scenario:?} on map {map:?}, found {:?}",
                    worlds.iter().collect::<Vec<_>>()
                ))
            }
        }
        VerificationCondition::NodeVisibility { node_name, visible } => {
            let actually_visible = nodes
                .iter()
                .any(|(name, _, _, visibility, _)| name.as_str() == node_name && visibility.get());
            if actually_visible == *visible {
                Ok(())
            } else {
                Err(format!(
                    "expected \"{node_name}\" to be {}",
                    if *visible { "visible" } else { "hidden" }
                ))
            }
        }
    };
    match result {
        Ok(()) => VerificationConditionState::Holds,
        Err(message) => VerificationConditionState::Broken(message),
    }
}

fn log_visible_named_nodes_for_failed_visibility(
    condition: &VerificationCondition,
    nodes: &VerificationUiNodeQuery,
) {
    let VerificationCondition::NodeVisibility { node_name, .. } = condition else {
        return;
    };
    let mut visible_names = nodes
        .iter()
        .filter(|(name, _, _, visibility, _)| {
            visibility.get() && !name.as_str().starts_with("ui visual")
        })
        .map(|(name, ..)| name.as_str())
        .collect::<Vec<_>>();
    visible_names.sort_unstable();
    visible_names.dedup();
    info!(target: "openzt2_verification", ?visible_names,
        matching_nodes = nodes.iter().filter(|(name, ..)| name.as_str() == node_name).count(),
        "visible named nodes at failed visibility check");
}

fn resolve_step_pointer_position(
    session: &mut VerificationJourneySession,
    pointer_target: &VerificationPointerTarget,
    catalogue: &VerificationCatalogueWheelInput,
    nodes: &VerificationUiNodeQuery,
    list_rows: &VerificationListRowQuery,
    names: &Query<&Name>,
    reach: &VerificationPointerReach,
    frame: u32,
) -> Result<Option<Vec2>, String> {
    let list_scope = match pointer_target {
        VerificationPointerTarget::CatalogueEntry {
            list_name: Some(name),
            ..
        } => Some(catalogue.resolve_active_hud_list(name)?),
        _ => None,
    };
    if let (
        Some(list),
        Some(target),
        VerificationPointerTarget::CatalogueEntry { definition, .. },
    ) = (list_scope, session.step_target, pointer_target)
    {
        if !list_rows.get(target).is_ok_and(|(_, row, _, entry)| {
            row.list == list && entry.is_some_and(|entry| entry.definition == *definition)
        }) {
            return Err("the row no longer matches the scoped catalogue target".to_owned());
        }
    }
    if let Some(position) = session.step_pointer_position {
        return Ok(Some(position));
    }
    // A node scrolled out of its list's view is shown but cannot be pressed, so
    // the step waits for it to scroll into view.
    let clipped_out_of_view = std::cell::Cell::new(false);
    let usable = |entity: Entity| {
        nodes
            .get(entity)
            .ok()
            .filter(|(_, computed, _, visibility, enabled)| {
                visibility.get()
                    && computed.size() != Vec2::ZERO
                    && enabled.is_none_or(|enabled| enabled.0)
            })
            .map(|(_, _, transform, _, _)| transform.translation)
            .filter(|position| {
                let clipped = reach.is_clipped_at(entity, *position);
                if clipped {
                    clipped_out_of_view.set(true);
                }
                !clipped
            })
    };
    let mut alternatives = Vec::new();
    let target = match pointer_target {
        VerificationPointerTarget::Position(position) => {
            session.step_pointer_position = Some(*position);
            return Ok(Some(*position));
        }
        VerificationPointerTarget::ListRow { list_name, index } => list_rows
            .iter()
            .find(|(_, row, visibility, _)| {
                row.index == *index
                    && visibility.get()
                    && names
                        .get(row.list)
                        .is_ok_and(|name| name.as_str() == list_name)
            })
            .and_then(|(entity, ..)| usable(entity).map(|position| (entity, position))),
        VerificationPointerTarget::CatalogueEntry { definition, .. } => {
            let mut matches = list_rows
                .iter()
                .filter(|(_, row, visibility, entry)| {
                    list_scope.is_none_or(|list| row.list == list)
                        && visibility.get()
                        && entry.is_some_and(|entry| entry.definition == *definition)
                })
                .filter_map(|(entity, ..)| usable(entity).map(|position| (entity, position)));
            let first = matches.next();
            if list_scope.is_some() && matches.next().is_some() {
                return Err("multiple usable rows in the scoped catalogue list match the requested definition".to_owned());
            }
            first
        }
        VerificationPointerTarget::NamedNode { node_name, index } => {
            let mut matches = reach
                .nodes_named(node_name)
                .into_iter()
                .filter_map(|entity| usable(entity).map(|position| (entity, position)))
                .collect::<Vec<_>>();
            matches.sort_by(|(_, left), (_, right)| {
                left.y.total_cmp(&right.y).then(left.x.total_cmp(&right.x))
            });
            // Authored names repeat across dialogs. When the first is asked for, the
            // others are tried in turn if a press would not reach it, and the first
            // is pressed if none would.
            if *index == 0 && matches.len() > 1 {
                alternatives = matches
                    .iter()
                    .skip(1)
                    .chain(matches.first())
                    .map(|(entity, _)| *entity)
                    .collect();
            }
            matches.get(*index).copied()
        }
    };
    match target {
        Some((entity, position)) => {
            session.step_pointer_position = Some(position);
            session.step_target = Some(entity);
            session.step_alternative_targets = alternatives;
            Ok(Some(position))
        }
        None if frame >= NAMED_NODE_WAIT_FRAME_LIMIT => Err(match pointer_target {
            VerificationPointerTarget::CatalogueEntry { definition_key, .. } => format!(
                "the open catalogue tab has no visible, enabled row for \"{definition_key}\" ({} catalogue rows visible)",
                list_rows
                    .iter()
                    .filter(|(_, row, visibility, entry)| {
                        list_scope.is_none_or(|list| row.list == list)
                            && visibility.get()
                            && entry.is_some()
                    })
                    .count()
            ),
            _ if clipped_out_of_view.get() => format!(
                "{pointer_target:?} is outside the view of the list that scrolls it; scroll it into view first"
            ),
            _ => format!("no visible, enabled UI node matched {pointer_target:?}"),
        }),
        None => Ok(None),
    }
}

fn summarize_frame_time_milliseconds(
    mut frame_time_milliseconds: Vec<f64>,
) -> super::verification_journey_state_types::VerificationFacts {
    let mut facts = super::verification_journey_state_types::VerificationFacts::new();
    if frame_time_milliseconds.is_empty() {
        return facts;
    }
    frame_time_milliseconds.sort_by(f64::total_cmp);
    let percentile = |percent: usize| {
        let index = (frame_time_milliseconds.len() * percent)
            .div_ceil(100)
            .saturating_sub(1);
        frame_time_milliseconds[index.min(frame_time_milliseconds.len() - 1)]
    };
    facts.insert("frame_time_median_ms".to_owned(), percentile(50));
    facts.insert("frame_time_p95_ms".to_owned(), percentile(95));
    facts.insert(
        "frame_time_worst_ms".to_owned(),
        frame_time_milliseconds[frame_time_milliseconds.len() - 1],
    );
    facts
}
