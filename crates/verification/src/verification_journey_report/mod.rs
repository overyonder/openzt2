use bevy::prelude::*;

use super::{
    verification_journey_state_types::{
        VerificationFailure, VerificationJourneyReport, VerificationJourneySession,
    },
    verification_journey_types::{
        VerificationExpectedFactValue, VerificationFactComparison, VerificationFactExpectation,
    },
};

pub(crate) fn compare_verification_fact(
    report: &VerificationJourneyReport,
    expectation: &VerificationFactExpectation,
) -> Result<(), String> {
    let actual = *report
        .current_facts
        .get(&expectation.fact)
        .ok_or_else(|| format!("unknown fact `{}`", expectation.fact))?;
    let expected = match &expectation.expected {
        VerificationExpectedFactValue::Number(value) => *value,
        VerificationExpectedFactValue::Snapshot {
            snapshot,
            fact,
            offset,
        } => {
            *report
                .snapshots
                .get(snapshot)
                .ok_or_else(|| format!("no snapshot named `{snapshot}`"))?
                .get(fact)
                .ok_or_else(|| format!("snapshot `{snapshot}` has no fact `{fact}`"))?
                + offset
        }
    };
    let holds = verification_comparison_holds(expectation.comparison, actual, expected);
    if holds {
        Ok(())
    } else {
        Err(format!(
            "expected {} {:?} {expected}, found {actual}",
            expectation.fact, expectation.comparison
        ))
    }
}

pub(crate) fn verification_comparison_holds(
    comparison: VerificationFactComparison,
    actual: f64,
    expected: f64,
) -> bool {
    match comparison {
        VerificationFactComparison::Equal => (actual - expected).abs() < f64::EPSILON,
        VerificationFactComparison::NotEqual => (actual - expected).abs() >= f64::EPSILON,
        VerificationFactComparison::Greater => actual > expected,
        VerificationFactComparison::GreaterOrEqual => actual >= expected,
        VerificationFactComparison::Less => actual < expected,
        VerificationFactComparison::LessOrEqual => actual <= expected,
    }
}

pub(crate) fn record_verification_failure(
    report: &mut VerificationJourneyReport,
    location: &str,
    message: String,
) {
    error!(target: "openzt2_verification", location, detail = %message, "verification failure");
    report.failures.push(VerificationFailure {
        location: location.to_owned(),
        message,
    });
}

/// Persists the current report without ending the journey or clearing its evidence.
pub(super) fn write_current_verification_journey_report(
    session: &VerificationJourneySession,
    report: &mut VerificationJourneyReport,
) {
    let journey = &session.journeys[session.journey_index].name;
    report.journey.clone_from(journey);
    report.passed = report.failures.is_empty();
    let report_path = session.output_root.join(journey).join("report.json");
    if let Err(error) = serde_json::to_vec_pretty(&*report)
        .map_err(std::io::Error::other)
        .and_then(|bytes| std::fs::write(&report_path, bytes))
    {
        error!(target: "openzt2_verification", %error, path = %report_path.display(),
            "could not write verification report");
        report.passed = false;
    }
}

/// Writes the current journey's `report.json` and starts an empty report for the next.
pub(crate) fn finish_current_verification_journey(
    session: &mut VerificationJourneySession,
    report: &mut VerificationJourneyReport,
) {
    write_current_verification_journey_report(session, report);
    // Frames per second show how fast the journey's frames ran.
    let (frames, seconds) = session
        .journey_started_at
        .map_or((0, 0.0), |(frame, started)| {
            (session.frames_run - frame, started.elapsed().as_secs_f64())
        });
    info!(target: "openzt2_verification", journey = %report.journey, passed = report.passed,
        failures = report.failures.len(), frames, seconds, "verification journey finished");
    session.any_journey_failed |= !report.passed;
    *report = VerificationJourneyReport::default();
}
