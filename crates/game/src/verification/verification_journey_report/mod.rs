use bevy::prelude::*;

use super::{
    verification_journey_state_types::{
        VerificationFailure, VerificationJourneyReport, VerificationJourneyRun,
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
        VerificationExpectedFactValue::Snapshot(snapshot) => *report
            .snapshots
            .get(snapshot)
            .ok_or_else(|| format!("no snapshot named `{snapshot}`"))?
            .get(&expectation.fact)
            .ok_or_else(|| format!("snapshot `{snapshot}` has no fact `{}`", expectation.fact))?,
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

/// Writes `report.json` into the journey's output directory and exits with the result.
pub(crate) fn finish_verification_journey(
    run: &mut VerificationJourneyRun,
    report: &mut VerificationJourneyReport,
    exit: &mut MessageWriter<AppExit>,
) {
    run.finished = true;
    report.journey.clone_from(&run.journey.name);
    report.passed = report.failures.is_empty();
    let report_path = run.output_directory.join("report.json");
    if let Err(error) = serde_json::to_vec_pretty(&*report)
        .map_err(std::io::Error::other)
        .and_then(|bytes| std::fs::write(&report_path, bytes))
    {
        error!(target: "openzt2_verification", %error, path = %report_path.display(),
            "could not write verification report");
        report.passed = false;
    }
    info!(target: "openzt2_verification", journey = %report.journey, passed = report.passed,
        failures = report.failures.len(), "verification journey finished");
    exit.write(if report.passed {
        AppExit::Success
    } else {
        AppExit::error()
    });
}
