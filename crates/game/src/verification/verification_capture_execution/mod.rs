use std::time::{Duration, Instant};

use bevy::{
    prelude::*,
    render::view::screenshot::{Screenshot, ScreenshotCaptured},
};

use super::{
    verification_capture_readiness::VerificationCaptureReadiness,
    verification_journey_execution::{collect_facts_when_ready, VerificationStepProgress},
    verification_journey_report::record_verification_failure,
    verification_journey_state_types::{
        VerificationCaptureProgress, VerificationJourneyReport, VerificationJourneySession,
        VerificationSessionStage,
    },
};

/// Frame limits count simulation ticks; capture completion also bounds wall time.
pub(super) const CAPTURE_READY_FRAME_LIMIT: u32 = 5_400;
const CAPTURE_READBACK_FRAME_LIMIT: u32 = 600;
pub(super) const CAPTURE_READY_WALL_LIMIT: Duration = Duration::from_secs(90);
const CAPTURE_COMPLETION_WALL_LIMIT: Duration = Duration::from_secs(10);

pub(super) fn advance_verification_capture(
    commands: &mut Commands,
    session: &mut VerificationJourneySession,
    report: &mut VerificationJourneyReport,
    readiness: &VerificationCaptureReadiness,
    name: &str,
    source_location: &str,
    frame: u32,
) -> VerificationStepProgress {
    if matches!(session.step_capture, VerificationCaptureProgress::Waiting) {
        session.step_capture = VerificationCaptureProgress::Preparing {
            started_at: Instant::now(),
        };
        info!(target: "openzt2_verification", journey = %session.journeys[session.journey_index].name,
            name, stage = ?session.stage, step = session.step_index, frame,
            "verification capture preparing");
    }
    match session.step_capture {
        VerificationCaptureProgress::Preparing { started_at }
            if readiness.uses_presentation_readiness()
                && session.stage != VerificationSessionStage::FinishAfterJourneyFailure
                && started_at.elapsed() >= CAPTURE_READY_WALL_LIMIT =>
        {
            // Last records the named predicates and extracts their render request
            // before this step is replaced by the failure-evidence capture.
            if !readiness.preparation_deadline_observed(session) {
                return VerificationStepProgress::Running;
            }
            VerificationStepProgress::Failed(format!(
                "capture {name} presentation was not ready after {:.1} wall seconds ({frame} frames)",
                started_at.elapsed().as_secs_f64()
            ))
        }
        // Failure evidence records the broken presentation, without requiring it
        // to settle successfully. Normal Full and Ready captures keep their gates.
        VerificationCaptureProgress::Preparing { started_at }
            if session.stage == VerificationSessionStage::FinishAfterJourneyFailure
                || readiness.is_ready(frame) =>
        {
            let path = session
                .output_root
                .join(&session.journeys[session.journey_index].name)
                .join(format!("{name}.png"));
            let location = source_location.to_owned();
            let entity = commands
                .spawn(Screenshot(session.capture_target.clone().unwrap_or_default()))
                .observe(move |captured: On<ScreenshotCaptured>,
                               mut session: ResMut<VerificationJourneySession>,
                               mut report: ResMut<VerificationJourneyReport>| {
                    if !matches!(session.step_capture, VerificationCaptureProgress::Requested { entity, .. } if entity == captured.entity) {
                        return;
                    }
                    let result = captured.image.clone().try_into_dynamic()
                        .map_err(std::io::Error::other)
                        .and_then(|image| image.to_rgb8().save_with_format(&path, image::ImageFormat::Png)
                            .map_err(std::io::Error::other));
                    let saved = result.is_ok();
                    if let Err(error) = result {
                        record_verification_failure(&mut report, &location,
                            format!("could not save capture {}: {error}", path.display()));
                    }
                    info!(target: "openzt2_verification", screenshot_entity = ?captured.entity,
                        path = %path.display(), saved, "verification capture readback finished");
                    session.step_capture = VerificationCaptureProgress::Finished { captured_at: Instant::now() };
                })
                .id();
            session.step_capture = VerificationCaptureProgress::Requested {
                entity,
                frame,
                requested_at: Instant::now(),
            };
            info!(target: "openzt2_verification", name, frames = frame,
                preparation_seconds = started_at.elapsed().as_secs_f64(), screenshot_entity = ?entity,
                "verification capture ready");
            VerificationStepProgress::Running
        }
        VerificationCaptureProgress::Preparing { .. } if frame >= CAPTURE_READY_FRAME_LIMIT => {
            VerificationStepProgress::Failed(format!(
                "capture {name} presentation was not ready after {frame} frames"
            ))
        }
        VerificationCaptureProgress::Requested {
            entity,
            frame: requested,
            requested_at,
        } if frame - requested >= CAPTURE_READBACK_FRAME_LIMIT
            || requested_at.elapsed() >= CAPTURE_COMPLETION_WALL_LIMIT =>
        {
            commands.entity(entity).try_despawn();
            VerificationStepProgress::Failed(format!(
                "capture {name} readback did not finish after {} frames ({:.1} wall seconds), screenshot entity {entity:?}",
                frame - requested,
                requested_at.elapsed().as_secs_f64()
            ))
        }
        VerificationCaptureProgress::Finished { captured_at }
            if captured_at.elapsed() >= CAPTURE_COMPLETION_WALL_LIMIT =>
        {
            VerificationStepProgress::Failed(format!(
                "capture {name} fact collection did not finish after {:.1} wall seconds (state {:?})",
                captured_at.elapsed().as_secs_f64(),
                session.fact_collection
            ))
        }
        VerificationCaptureProgress::Finished { .. } => {
            if collect_facts_when_ready(session) {
                report
                    .snapshots
                    .insert(name.to_owned(), report.current_facts.clone());
                VerificationStepProgress::Complete
            } else {
                VerificationStepProgress::Running
            }
        }
        _ => VerificationStepProgress::Running,
    }
}
