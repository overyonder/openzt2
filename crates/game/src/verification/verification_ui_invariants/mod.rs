use bevy::prelude::*;

use crate::plugins::ui::{
    scrollbar_value_and_scroll_position_synchronization::UiScrollDecoration, slider::UiSliderAxis,
};

use super::{
    verification_journey_report::record_verification_failure,
    verification_journey_state_types::{VerificationJourneyReport, VerificationJourneyRun},
};

/// Checks that hold on every frame of every journey. Each is reported once.
pub(crate) fn record_broken_verification_ui_invariants(
    run: Res<VerificationJourneyRun>,
    mut report: ResMut<VerificationJourneyReport>,
    interactions: Query<(&Interaction, &InheritedVisibility)>,
    scroll_decorations: Query<(&UiScrollDecoration, &Visibility, &InheritedVisibility)>,
    computed_nodes: Query<&ComputedNode>,
) {
    if run.finished {
        return;
    }
    let location = run
        .journey
        .steps
        .get(run.step_index)
        .map_or_else(String::new, |step| step.source_location.clone());
    let hovered = interactions
        .iter()
        .filter(|(interaction, visibility)| {
            visibility.get() && **interaction == Interaction::Hovered
        })
        .count();
    if hovered > 1 && report.recorded_invariants.insert("single hovered control") {
        record_verification_failure(
            &mut report,
            &location,
            format!("{hovered} visible controls are hovered at once"),
        );
    }
    let decoration_without_overflow =
        scroll_decorations
            .iter()
            .any(|(decoration, visibility, inherited)| {
                if matches!(visibility, Visibility::Hidden) || !inherited.get() {
                    return false;
                }
                computed_nodes.get(decoration.owner).is_ok_and(|computed| {
                    let overflow = computed.content_size() - computed.size();
                    let has_overflow = match decoration.axis {
                        UiSliderAxis::Horizontal => overflow.x > 0.5,
                        UiSliderAxis::Vertical => overflow.y > 0.5,
                        UiSliderAxis::Both => overflow.max_element() > 0.5,
                    };
                    !has_overflow
                })
            });
    if decoration_without_overflow
        && report
            .recorded_invariants
            .insert("scrollbar needs overflow")
    {
        record_verification_failure(
            &mut report,
            &location,
            "a scrollbar is visible on content that does not overflow".to_owned(),
        );
    }
}
