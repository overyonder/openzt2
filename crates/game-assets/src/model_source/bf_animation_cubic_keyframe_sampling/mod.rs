use bevy::curve::cubic_splines::CubicSegment;

type BlueFangScalarKey<const N: usize> = (u16, [f32; N], [f32; N], [f32; N]);

#[derive(Clone)]
pub(super) struct BlueFangScalarCurve<const N: usize> {
    segments: Vec<(f32, [CubicSegment<f32>; N])>,
}

impl<const N: usize> BlueFangScalarCurve<N> {
    pub(super) fn new(mut keys: Vec<BlueFangScalarKey<N>>, cubic: bool) -> Self {
        keys.sort_by_key(|key| key.0);
        keys.dedup_by_key(|key| key.0);
        let segments = keys
            .iter()
            .enumerate()
            .map(|(index, left)| {
                let right = keys.get(index + 1).unwrap_or(left);
                (
                    f32::from(left.0) / 1000.0,
                    std::array::from_fn(|axis| {
                        let change = right.1[axis] - left.1[axis];
                        let coeff = if cubic && right.0 != left.0 {
                            let outgoing = left.3[axis];
                            let incoming = right.2[axis];
                            [
                                left.1[axis],
                                outgoing,
                                (-2.0_f32).mul_add(outgoing, 3.0 * change) - incoming,
                                (-2.0_f32).mul_add(change, outgoing + incoming),
                            ]
                        } else {
                            [left.1[axis], change, 0.0, 0.0]
                        };
                        CubicSegment { coeff }
                    }),
                )
            })
            .collect();
        Self { segments }
    }

    pub(super) fn end_seconds(&self) -> Option<f32> {
        self.segments.last().map(|segment| segment.0)
    }

    pub(super) fn sample(&self, time: f32) -> [f32; N] {
        if self.segments.is_empty() {
            return [0.0; N];
        }
        let index = self
            .segments
            .partition_point(|segment| segment.0 <= time)
            .saturating_sub(1);
        let (start, segments) = &self.segments[index];
        let fraction = self
            .segments
            .get(index + 1)
            .map_or(0.0, |next| (time - start).max(0.0) / (next.0 - start));
        std::array::from_fn(|axis| segments[axis].position(fraction))
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn normalized_tangents_follow_the_authored_hermite_curve() {
        let curve = super::BlueFangScalarCurve::new(
            vec![(0, [0.0], [0.0], [2.0]), (1000, [1.0], [0.0], [0.0])],
            true,
        );
        assert_eq!(
            [0.0, 0.25, 0.5, 0.75, 1.0].map(|time| curve.sample(time)[0]),
            [0.0, 0.4375, 0.75, 0.9375, 1.0]
        );
    }
}
