use bevy::prelude::{Entity, Vec3};

#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct FossilSonarCandidateScore {
    pub(super) entity: Entity,
    pub(super) distance_squared: f32,
    pub(super) score: f32,
}

/// Chooses the strongest fossil-sonar return without materialising a candidate
/// list. The policy inputs are the final consumer values: squared distance
/// limits and the cosine of half the authored view-cone angle.
///
/// The excluded entity is the mode's retained artifact. The original scorer
/// omits that exact artifact before measuring candidates, uses horizontal
/// camera/candidate directions, multiplies the normalized distance and cone
/// factors, and replaces the winner only on a strictly greater score.
pub(super) fn select_highest_scoring_fossil_sonar_candidate(
    camera_translation: Vec3,
    camera_forward: Vec3,
    excluded_artifact: Option<Entity>,
    minimum_distance_squared: f32,
    maximum_distance_squared: f32,
    minimum_view_direction_dot_product: f32,
    candidate_artifact_translations: impl IntoIterator<Item = (Entity, Vec3)>,
) -> Option<FossilSonarCandidateScore> {
    let horizontal_camera_forward =
        Vec3::new(camera_forward.x, 0.0, camera_forward.z).try_normalize()?;
    let distance_score_span = maximum_distance_squared - minimum_distance_squared;
    let view_cone_score_span = 1.0 - minimum_view_direction_dot_product;
    if !camera_translation.is_finite()
        || !minimum_distance_squared.is_finite()
        || !maximum_distance_squared.is_finite()
        || !minimum_view_direction_dot_product.is_finite()
        || distance_score_span <= 0.0
        || view_cone_score_span <= 0.0
    {
        return None;
    }

    candidate_artifact_translations
        .into_iter()
        .filter(|(entity, _)| Some(*entity) != excluded_artifact)
        .filter_map(|(entity, translation)| {
            let horizontal_camera_to_artifact = Vec3::new(
                translation.x - camera_translation.x,
                0.0,
                translation.z - camera_translation.z,
            );
            let distance_squared = horizontal_camera_to_artifact.length_squared();
            if !distance_squared.is_finite() || distance_squared >= maximum_distance_squared {
                return None;
            }
            let view_direction_dot_product =
                horizontal_camera_forward.dot(horizontal_camera_to_artifact.try_normalize()?);
            if !view_direction_dot_product.is_finite()
                || view_direction_dot_product < minimum_view_direction_dot_product
            {
                return None;
            }
            let distance_score =
                (maximum_distance_squared - distance_squared) / distance_score_span;
            let view_cone_score = (view_direction_dot_product - minimum_view_direction_dot_product)
                / view_cone_score_span;
            let combined_score = distance_score * view_cone_score;
            (combined_score > 0.0).then_some(FossilSonarCandidateScore {
                entity,
                distance_squared,
                score: combined_score,
            })
        })
        .reduce(|highest_scoring_candidate, candidate| {
            if candidate.score > highest_scoring_candidate.score {
                candidate
            } else {
                highest_scoring_candidate
            }
        })
        .map(|candidate| FossilSonarCandidateScore {
            score: candidate.score.min(1.0),
            ..candidate
        })
}
