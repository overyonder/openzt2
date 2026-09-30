use bevy::{
    prelude::*,
    render::view::screenshot::{save_to_disk, Screenshot, ScreenshotCaptured},
};

use crate::plugins::ui::{
    authored_reusable_list_and_table_runtime_types::UiListRow,
    authored_ui_interaction_enabled_state::UiInteractionEnabled,
};
use crate::plugins::world_spawn::{
    selected_world_identity::SelectedWorldIdentity, world_membership_types::WorldRoot,
};

use super::{
    verification_game_phase_readiness::{game_phase_order, VerificationGamePhaseReadiness},
    verification_input_injection::VerificationInputInjection,
    verification_journey_report::{
        compare_verification_fact, finish_verification_journey, record_verification_failure,
        verification_comparison_holds,
    },
    verification_journey_state_types::{
        VerificationFactCollectionState, VerificationJourneyReport, VerificationJourneyRun,
    },
    verification_journey_types::{VerificationJourneyAction, VerificationPointerTarget},
};

/// 90 seconds at 60 Hz, long enough for a map to load on a slow machine.
const PHASE_WAIT_FRAME_LIMIT: u32 = 5_400;
const NAMED_NODE_WAIT_FRAME_LIMIT: u32 = 600;
/// Frames of stable presentation before a screenshot, so pending GPU assets can arrive.
const CAPTURE_SETTLE_FRAMES: u32 = 30;
const DRAG_MOVEMENT_FRAMES: u32 = 8;

type VerificationUiNodeQuery<'w, 's> = Query<
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

enum VerificationStepProgress {
    Running,
    Complete,
    Failed(String),
}

#[allow(clippy::needless_pass_by_value)]
pub(crate) fn advance_verification_journey_by_one_frame(
    mut commands: Commands,
    mut run: ResMut<VerificationJourneyRun>,
    mut report: ResMut<VerificationJourneyReport>,
    readiness: VerificationGamePhaseReadiness,
    nodes: VerificationUiNodeQuery,
    list_rows: Query<(Entity, &UiListRow, &InheritedVisibility)>,
    worlds: Query<&SelectedWorldIdentity, With<WorldRoot>>,
    names: Query<&Name>,
    texts: Query<(
        &Name,
        Option<&Text>,
        Option<&bevy::text::EditableText>,
        &InheritedVisibility,
    )>,
    mut input: VerificationInputInjection,
    mut exit: MessageWriter<AppExit>,
) {
    if run.finished {
        return;
    }
    let Some(target) = run
        .capture_target
        .as_ref()
        .and_then(|target| target.normalize(Some(input.primary_window_entity())))
    else {
        return;
    };
    let Some(step) = run.journey.steps.get(run.step_index).cloned() else {
        finish_verification_journey(&mut run, &mut report, &mut exit);
        return;
    };
    let frame = run.step_frame;
    let mut pointer_input_written = false;
    let progress = match &step.action {
        VerificationJourneyAction::WaitForPhase(phase) => {
            if readiness.phase_is_ready_for_input(*phase) {
                VerificationStepProgress::Complete
            } else if game_phase_order(readiness.current_phase()) > game_phase_order(*phase) {
                VerificationStepProgress::Failed(format!(
                    "game reached {:?} before {phase:?} was ready",
                    readiness.current_phase()
                ))
            } else if frame >= PHASE_WAIT_FRAME_LIMIT {
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
        VerificationJourneyAction::MovePointer(pointer_target) => {
            match resolve_step_pointer_position(
                &mut run,
                pointer_target,
                &nodes,
                &list_rows,
                &names,
                frame,
            ) {
                Err(message) => VerificationStepProgress::Failed(message),
                Ok(None) => VerificationStepProgress::Running,
                Ok(Some(position)) => {
                    input.move_pointer(&target, run.pointer_position, position);
                    run.pointer_position = position;
                    pointer_input_written = true;
                    VerificationStepProgress::Complete
                }
            }
        }
        VerificationJourneyAction::Click(pointer_target) => {
            match resolve_step_pointer_position(
                &mut run,
                pointer_target,
                &nodes,
                &list_rows,
                &names,
                frame,
            ) {
                Err(message) => VerificationStepProgress::Failed(message),
                Ok(None) => VerificationStepProgress::Running,
                Ok(Some(position)) => {
                    // Picking needs a frame to hover the target before the press.
                    let resolved_frame = run.step_resolved_frame.get_or_insert(frame);
                    match frame - *resolved_frame {
                        0 => {
                            input.move_pointer(&target, run.pointer_position, position);
                            run.pointer_position = position;
                            pointer_input_written = true;
                            VerificationStepProgress::Running
                        }
                        2 => {
                            input.press_or_release_primary_button(&target, position, true);
                            pointer_input_written = true;
                            VerificationStepProgress::Running
                        }
                        4 => {
                            input.press_or_release_primary_button(&target, position, false);
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
            match resolve_step_pointer_position(&mut run, from, &nodes, &list_rows, &names, frame) {
                Err(message) => VerificationStepProgress::Failed(message),
                Ok(None) => VerificationStepProgress::Running,
                Ok(Some(start)) => {
                    let resolved_frame = *run.step_resolved_frame.get_or_insert(frame);
                    let elapsed = frame - resolved_frame;
                    let release_frame = 4 + DRAG_MOVEMENT_FRAMES;
                    if elapsed == 0 {
                        input.move_pointer(&target, run.pointer_position, start);
                        run.pointer_position = start;
                        pointer_input_written = true;
                    } else if elapsed == 2 {
                        input.press_or_release_primary_button(&target, start, true);
                        pointer_input_written = true;
                    } else if (3..release_frame).contains(&elapsed) {
                        #[allow(clippy::cast_precision_loss)]
                        let fraction = (elapsed - 2) as f32 / DRAG_MOVEMENT_FRAMES as f32;
                        let position = start.lerp(*to, fraction.min(1.0));
                        input.move_pointer(&target, run.pointer_position, position);
                        run.pointer_position = position;
                        pointer_input_written = true;
                    } else if elapsed == release_frame {
                        input.move_pointer(&target, run.pointer_position, *to);
                        run.pointer_position = *to;
                        input.press_or_release_primary_button(&target, *to, false);
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
        VerificationJourneyAction::Key { key_code, repeat } => {
            // Press on even frames and release on odd ones, once per repeat.
            if frame < repeat * 2 {
                input.press_or_release_key(*key_code, frame % 2 == 0);
                VerificationStepProgress::Running
            } else if frame < repeat * 2 + 2 {
                VerificationStepProgress::Running
            } else {
                VerificationStepProgress::Complete
            }
        }
        VerificationJourneyAction::TypeText(text) => {
            // One character per two frames: press, then release.
            let characters = text.chars().collect::<Vec<_>>();
            let character_index = (frame / 2) as usize;
            if let Some(character) = characters.get(character_index) {
                input.press_or_release_text_character(*character, frame % 2 == 0);
                VerificationStepProgress::Running
            } else {
                VerificationStepProgress::Complete
            }
        }
        VerificationJourneyAction::Wheel { lines, repeat } => {
            // One wheel event every other frame, as separate notches arrive natively.
            if frame < repeat * 2 {
                if frame % 2 == 0 {
                    input.scroll_wheel_lines(*lines);
                }
                VerificationStepProgress::Running
            } else {
                VerificationStepProgress::Complete
            }
        }
        VerificationJourneyAction::Snapshot(name) => {
            if collect_facts_when_ready(&mut run) {
                let facts = report.current_facts.clone();
                report.snapshots.insert(name.clone(), facts);
                VerificationStepProgress::Complete
            } else {
                VerificationStepProgress::Running
            }
        }
        VerificationJourneyAction::Capture(name) => {
            if frame == CAPTURE_SETTLE_FRAMES {
                run.screenshot_pending = true;
                let path = run.output_directory.join(format!("{name}.png"));
                commands
                    .spawn(Screenshot(run.capture_target.clone().unwrap_or_default()))
                    .observe(save_to_disk(path))
                    .observe(
                        |_: On<ScreenshotCaptured>, mut run: ResMut<VerificationJourneyRun>| {
                            run.screenshot_pending = false;
                        },
                    );
            }
            if frame >= CAPTURE_SETTLE_FRAMES
                && !run.screenshot_pending
                && collect_facts_when_ready(&mut run)
            {
                let facts = report.current_facts.clone();
                report.snapshots.insert(name.clone(), facts);
                VerificationStepProgress::Complete
            } else {
                VerificationStepProgress::Running
            }
        }
        VerificationJourneyAction::MeasureFrameTime(frames) => {
            run.measuring_frame_work = frame < *frames;
            if frame > *frames {
                report.frame_time_facts = summarize_frame_time_milliseconds(std::mem::take(
                    &mut run.frame_time_milliseconds,
                ));
                VerificationStepProgress::Complete
            } else {
                VerificationStepProgress::Running
            }
        }
        VerificationJourneyAction::ExpectFact(expectation) => {
            if collect_facts_when_ready(&mut run) {
                if let Err(message) = compare_verification_fact(&report, expectation) {
                    record_verification_failure(&mut report, &step.source_location, message);
                }
                VerificationStepProgress::Complete
            } else {
                VerificationStepProgress::Running
            }
        }
        VerificationJourneyAction::ExpectListRowCount {
            list_name,
            comparison,
            expected,
        } => {
            let visible_rows = list_rows
                .iter()
                .filter(|(_, row, visibility)| {
                    visibility.get()
                        && names
                            .get(row.list)
                            .is_ok_and(|name| name.as_str() == list_name)
                })
                .count();
            let actual = f64::from(u32::try_from(visible_rows).unwrap_or(u32::MAX));
            if !verification_comparison_holds(*comparison, actual, *expected) {
                record_verification_failure(
                    &mut report,
                    &step.source_location,
                    format!("expected {comparison:?} {expected} visible rows in \"{list_name}\", found {actual}"),
                );
            }
            VerificationStepProgress::Complete
        }
        VerificationJourneyAction::ExpectNodeText {
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
            if !shown_texts.iter().any(|shown| shown == expected) {
                record_verification_failure(
                    &mut report,
                    &step.source_location,
                    format!("expected \"{node_name}\" to show {expected:?}, found {shown_texts:?}"),
                );
            }
            VerificationStepProgress::Complete
        }
        VerificationJourneyAction::ExpectCampaignWorld { scenario, map } => {
            let expected_scenario = openzt2_game_data::AssetId::from_key(scenario);
            let expected_map = openzt2_game_data::AssetId::from_key(map);
            if !worlds.iter().any(|world| {
                world.mode == crate::game_session_types::WorldSessionMode::Campaign
                    && world.requested == expected_scenario
                    && world.map == expected_map
            }) {
                record_verification_failure(
                    &mut report,
                    &step.source_location,
                    format!(
                        "expected campaign scenario {scenario:?} on map {map:?}, found {:?}",
                        worlds.iter().collect::<Vec<_>>()
                    ),
                );
            }
            VerificationStepProgress::Complete
        }
        VerificationJourneyAction::ExpectNodeVisibility { node_name, visible } => {
            let actually_visible = nodes
                .iter()
                .any(|(name, _, _, visibility, _)| name.as_str() == node_name && visibility.get());
            if actually_visible != *visible {
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
                record_verification_failure(
                    &mut report,
                    &step.source_location,
                    format!(
                        "expected \"{node_name}\" to be {}",
                        if *visible { "visible" } else { "hidden" }
                    ),
                );
            }
            VerificationStepProgress::Complete
        }
    };
    if !pointer_input_written {
        input.resubmit_stationary_pointer(&target, run.pointer_position);
    }
    match progress {
        VerificationStepProgress::Running => run.step_frame += 1,
        VerificationStepProgress::Complete => {
            debug!(target: "openzt2_verification", location = %step.source_location, "journey step complete");
            run.step_index += 1;
            run.step_frame = 0;
            run.step_pointer_position = None;
            run.step_resolved_frame = None;
        }
        VerificationStepProgress::Failed(message) => {
            record_verification_failure(&mut report, &step.source_location, message);
            finish_verification_journey(&mut run, &mut report, &mut exit);
        }
    }
}

pub(crate) fn mark_verification_frame_work_start(mut run: ResMut<VerificationJourneyRun>) {
    run.frame_work_started = Some(std::time::Instant::now());
}

/// Frame work excludes the 60 Hz pacing sleep, so it shows headroom as well as overruns.
pub(crate) fn record_verification_frame_work_time(mut run: ResMut<VerificationJourneyRun>) {
    if run.measuring_frame_work {
        if let Some(started) = run.frame_work_started {
            let milliseconds = started.elapsed().as_secs_f64() * 1000.0;
            run.frame_time_milliseconds.push(milliseconds);
        }
    }
}

/// Requests facts on the first call and returns true once the collector has filled them.
fn collect_facts_when_ready(run: &mut VerificationJourneyRun) -> bool {
    match run.fact_collection {
        VerificationFactCollectionState::Idle => {
            run.fact_collection = VerificationFactCollectionState::Requested;
            false
        }
        VerificationFactCollectionState::Requested => false,
        VerificationFactCollectionState::Collected => {
            run.fact_collection = VerificationFactCollectionState::Idle;
            true
        }
    }
}

fn resolve_step_pointer_position(
    run: &mut VerificationJourneyRun,
    pointer_target: &VerificationPointerTarget,
    nodes: &VerificationUiNodeQuery,
    list_rows: &Query<(Entity, &UiListRow, &InheritedVisibility)>,
    names: &Query<&Name>,
    frame: u32,
) -> Result<Option<Vec2>, String> {
    if let Some(position) = run.step_pointer_position {
        return Ok(Some(position));
    }
    let position = match pointer_target {
        VerificationPointerTarget::Position(position) => Some(*position),
        VerificationPointerTarget::ListRow { list_name, index } => list_rows
            .iter()
            .find(|(_, row, visibility)| {
                row.index == *index
                    && visibility.get()
                    && names
                        .get(row.list)
                        .is_ok_and(|name| name.as_str() == list_name)
            })
            .and_then(|(entity, _, _)| nodes.get(entity).ok())
            .filter(|(_, computed, _, visibility, enabled)| {
                visibility.get()
                    && computed.size() != Vec2::ZERO
                    && enabled.is_none_or(|enabled| enabled.0)
            })
            .map(|(_, _, transform, _, _)| transform.translation),
        VerificationPointerTarget::NamedNode { node_name, index } => {
            let mut matches = nodes
                .iter()
                .filter(|(name, computed, _, visibility, enabled)| {
                    name.as_str() == node_name
                        && visibility.get()
                        && computed.size() != Vec2::ZERO
                        && enabled.is_none_or(|enabled| enabled.0)
                })
                .map(|(_, _, transform, _, _)| transform.translation)
                .collect::<Vec<_>>();
            matches
                .sort_by(|left, right| left.y.total_cmp(&right.y).then(left.x.total_cmp(&right.x)));
            matches.get(*index).copied()
        }
    };
    match position {
        Some(position) => {
            run.step_pointer_position = Some(position);
            Ok(Some(position))
        }
        None if frame >= NAMED_NODE_WAIT_FRAME_LIMIT => Err(format!(
            "no visible, enabled UI node matched {pointer_target:?}"
        )),
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
