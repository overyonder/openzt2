use bevy::prelude::*;
use openzt2_game_data::{world_definitions::fences_and_gates::FenceDefinition, AssetId};

use super::topology_presentation_types::FenceSegmentPrefab;

/// Finds the reciprocal, unambiguous corner pairing used by the authored
/// half-curve meshes. Both meshes run from their outer endpoint to the shared
/// corner. A straight continuation is not a curve candidate; competing turns
/// at either end leave the segment straight.
pub(super) fn select_reciprocal_fence_curve_partners(
    segments: &[(AssetId, IVec3, IVec3)],
) -> Vec<Option<(IVec3, IVec3, IVec3)>> {
    let mut endpoints = bevy::platform::collections::HashMap::<_, Vec<usize>>::new();
    for (index, &(definition, first, second)) in segments.iter().enumerate() {
        endpoints
            .entry((definition, first))
            .or_default()
            .push(index);
        endpoints
            .entry((definition, second))
            .or_default()
            .push(index);
    }
    let candidates = segments
        .iter()
        .enumerate()
        .map(|(index, &(definition, first, second))| {
            let mut candidate = None;
            for (outer, corner) in [(first, second), (second, first)] {
                let adjacent = &endpoints[&(definition, corner)];
                if adjacent.len() > 2 {
                    return None;
                }
                for &other_index in adjacent {
                    if other_index == index {
                        continue;
                    }
                    let (_, a, b) = segments[other_index];
                    let other = if a == corner { b } else { a };
                    if authored_fence_segment_turn(
                        (corner - outer).xy().signum(),
                        (other - corner).xy().signum(),
                    )
                    .is_none()
                    {
                        continue;
                    }
                    if candidate
                        .replace((other_index, outer, corner, other))
                        .is_some()
                    {
                        return None;
                    }
                }
            }
            candidate
        })
        .collect::<Vec<_>>();
    candidates
        .iter()
        .enumerate()
        .map(|(index, candidate)| {
            let &(other_index, outer, corner, other) = candidate.as_ref()?;
            (candidates[other_index].as_ref()?.0 == index).then_some((outer, corner, other))
        })
        .collect()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum AuthoredFenceSegmentTurn {
    NinetyDegrees,
    OneHundredThirtyFiveDegrees,
}

/// Chooses one of the six authored fence binders from the current edge and
/// the edge which leaves its second endpoint. The source names describe the
/// current segment orientation first (`fence90` or `fence45`) and the corner's
/// interior angle second. An absent optional curve falls back to the matching
/// straight segment from the same fence definition.
pub(super) fn select_authored_fence_segment_prefab_for_adjacent_topology(
    definition: &FenceDefinition,
    first_cell: IVec3,
    second_cell: IVec3,
    next_cell: Option<IVec3>,
) -> FenceSegmentPrefab {
    let current_direction = (second_cell - first_cell).xy().signum();
    let current_segment_is_diagonal = current_direction.x != 0 && current_direction.y != 0;
    let segment_prefabs = &definition.segments;
    let straight_prefab = if current_segment_is_diagonal {
        segment_prefabs.diagonal_straight
    } else {
        segment_prefabs.cardinal_straight
    };
    let selected_curve_prefab = next_cell
        .and_then(|next_cell| {
            authored_fence_segment_turn(current_direction, (next_cell - second_cell).xy().signum())
        })
        .map(|turn| match (current_segment_is_diagonal, turn) {
            (false, AuthoredFenceSegmentTurn::NinetyDegrees) => segment_prefabs.cardinal_curve_90,
            (true, AuthoredFenceSegmentTurn::NinetyDegrees) => segment_prefabs.diagonal_curve_90,
            (false, AuthoredFenceSegmentTurn::OneHundredThirtyFiveDegrees) => {
                segment_prefabs.cardinal_curve_135
            }
            (true, AuthoredFenceSegmentTurn::OneHundredThirtyFiveDegrees) => {
                segment_prefabs.diagonal_curve_135
            }
        })
        .filter(|prefab| *prefab != AssetId::default());
    let turn_cross = next_cell.map_or(0, |next| {
        current_direction.perp_dot((next - second_cell).xy().signum())
    });
    // Source headings increase clockwise. After source Y -> Bevy -Z,
    // cardinal curves mirror on a negative XZ turn; diagonal 135 curves
    // use the opposite authored bend. Native diagonal 90 follows cardinal.
    let diagonal_135 = current_segment_is_diagonal
        && next_cell
            .is_some_and(|next| current_direction.dot((next - second_cell).xy().signum()) > 0);
    FenceSegmentPrefab {
        asset: selected_curve_prefab.unwrap_or(straight_prefab),
        mirror_source_y: selected_curve_prefab.is_some()
            && if diagonal_135 {
                turn_cross > 0
            } else {
                turn_cross < 0
            },
    }
}

fn authored_fence_segment_turn(
    current_direction: IVec2,
    outgoing_direction: IVec2,
) -> Option<AuthoredFenceSegmentTurn> {
    if current_direction == IVec2::ZERO
        || outgoing_direction == IVec2::ZERO
        || current_direction == outgoing_direction
    {
        return None;
    }
    let dot_product = current_direction.dot(outgoing_direction);
    let cross_product = current_direction.perp_dot(outgoing_direction);
    if cross_product == 0 || dot_product < 0 {
        None
    } else if dot_product == 0 {
        Some(AuthoredFenceSegmentTurn::NinetyDegrees)
    } else {
        Some(AuthoredFenceSegmentTurn::OneHundredThirtyFiveDegrees)
    }
}

#[cfg(test)]
mod tests {
    use openzt2_game_data::world_definitions::fences_and_gates::{
        FenceGatePolicy, FenceSegmentPrefabs, FenceTraversalBlockingFlags,
    };

    use super::*;

    #[test]
    fn reciprocal_corner_reverses_second_segment_and_mirrors_its_authored_bend() {
        let definition = fence_definition_with_six_distinct_segment_prefabs();
        let first = IVec3::ZERO;
        let corner = IVec3::new(3, 0, 0);
        let last = IVec3::new(3, 3, 0);
        let partners = select_reciprocal_fence_curve_partners(&[
            (definition.id, first, corner),
            (definition.id, corner, last),
        ]);
        assert_eq!(
            partners,
            vec![Some((first, corner, last)), Some((last, corner, first))]
        );
        let a = select_authored_fence_segment_prefab_for_adjacent_topology(
            &definition,
            first,
            corner,
            Some(last),
        );
        let b = select_authored_fence_segment_prefab_for_adjacent_topology(
            &definition,
            last,
            corner,
            Some(first),
        );
        assert!(!a.mirror_source_y);
        assert!(b.mirror_source_y);
        // Authored asphalt curve90 end_post after NIF -> Bevy conversion.
        let endpoint = Vec3::new(2.153_149_6, 0.0, 0.85);
        let second_endpoint = Vec3::new(3.0, 0.0, 3.0)
            + Quat::from_rotation_y(std::f32::consts::FRAC_PI_2)
                * (endpoint * Vec3::new(1.0, 1.0, -1.0));
        assert!(endpoint.distance(second_endpoint) < 0.005);
    }

    #[test]
    fn straight_continuations_and_competing_corners_do_not_choose_arbitrary_partners() {
        let id = AssetId::from_virtual_path("test/curb");
        let straight = [(id, IVec3::ZERO, IVec3::X), (id, IVec3::X, IVec3::X * 2)];
        assert_eq!(
            select_reciprocal_fence_curve_partners(&straight),
            vec![None, None]
        );
        let two_corners = [
            (id, IVec3::ZERO, IVec3::X),
            (id, IVec3::X, IVec3::new(1, 1, 0)),
            (id, IVec3::new(1, 1, 0), IVec3::new(2, 1, 0)),
        ];
        assert_eq!(
            select_reciprocal_fence_curve_partners(&two_corners),
            vec![None, None, None]
        );
    }

    #[test]
    fn diagonal_135_uses_opposite_mirror_polarity_to_cardinal_curve() {
        let definition = fence_definition_with_six_distinct_segment_prefabs();
        for sign in [-1, 1] {
            let selected = select_authored_fence_segment_prefab_for_adjacent_topology(
                &definition,
                IVec3::ZERO,
                IVec3::new(1, sign, 0),
                Some(IVec3::new(2, sign, 0)),
            );
            assert_eq!(selected.mirror_source_y, sign < 0);
        }
    }

    fn fence_definition_with_six_distinct_segment_prefabs() -> FenceDefinition {
        FenceDefinition {
            id: AssetId::from_virtual_path("test/fence"),
            object: AssetId::default(),
            segment_length_cm: 300,
            height_cm: 200,
            strength: 1,
            blocks: FenceTraversalBlockingFlags::default(),
            gate: AssetId::default(),
            post_prefab: AssetId::default(),
            segments: FenceSegmentPrefabs {
                cardinal_straight: AssetId::from_virtual_path("test/fence90"),
                diagonal_straight: AssetId::from_virtual_path("test/fence45"),
                cardinal_curve_90: AssetId::from_virtual_path("test/fence90curve90"),
                diagonal_curve_90: AssetId::from_virtual_path("test/fence45curve90"),
                cardinal_curve_135: AssetId::from_virtual_path("test/fence90curve135"),
                diagonal_curve_135: AssetId::from_virtual_path("test/fence45curve135"),
            },
            gate_policy: FenceGatePolicy {
                prefab: AssetId::default(),
                open_animation: AssetId::default(),
                close_animation: AssetId::default(),
                trigger_distance_cm: 0,
                auto_close_ticks: 0,
            },
        }
    }

    #[test]
    fn selects_all_six_authored_fence_segment_variants() {
        let definition = fence_definition_with_six_distinct_segment_prefabs();
        let select = |first, second, next| {
            select_authored_fence_segment_prefab_for_adjacent_topology(
                &definition,
                first,
                second,
                next,
            )
            .asset
        };

        assert_eq!(
            select(IVec3::ZERO, IVec3::X, Some(IVec3::X * 2)),
            definition.segments.cardinal_straight
        );
        assert_eq!(
            select(IVec3::ZERO, IVec3::new(1, 1, 0), None),
            definition.segments.diagonal_straight
        );
        assert_eq!(
            select(IVec3::ZERO, IVec3::X, Some(IVec3::new(1, 1, 0))),
            definition.segments.cardinal_curve_90
        );
        assert_eq!(
            select(IVec3::ZERO, IVec3::new(1, 1, 0), Some(IVec3::new(0, 2, 0)),),
            definition.segments.diagonal_curve_90
        );
        assert_eq!(
            select(IVec3::ZERO, IVec3::X, Some(IVec3::new(2, 1, 0))),
            definition.segments.cardinal_curve_135
        );
        assert_eq!(
            select(IVec3::ZERO, IVec3::new(1, 1, 0), Some(IVec3::new(2, 1, 0)),),
            definition.segments.diagonal_curve_135
        );
    }
}
