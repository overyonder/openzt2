//! Samples normalized Hermite tangents at the playback clock's millisecond resolution.

pub(super) fn sample_cubic_keyframes<const N: usize>(
    mut keys: Vec<(u16, [f32; N], [f32; N], [f32; N])>,
) -> Vec<(u16, [f32; N])> {
    keys.sort_by_key(|key| key.0);
    keys.dedup_by_key(|key| key.0);
    let mut samples = Vec::new();
    for pair in keys.windows(2) {
        let (left, right) = (pair[0], pair[1]);
        for time in left.0..right.0 {
            let t = f32::from(time - left.0) / f32::from(right.0 - left.0);
            samples.push((
                time,
                std::array::from_fn(|axis| {
                    let difference = right.1[axis] - left.1[axis];
                    let outgoing = left.3[axis];
                    let incoming = right.2[axis];
                    let cubic = (-2.0_f32).mul_add(difference, outgoing + incoming);
                    let quadratic = (-2.0_f32).mul_add(outgoing, 3.0 * difference) - incoming;
                    cubic
                        .mul_add(t, quadratic)
                        .mul_add(t, outgoing)
                        .mul_add(t, left.1[axis])
                }),
            ));
        }
    }
    if let Some(last) = keys.last() {
        samples.push((last.0, last.1));
    }
    samples
}

#[cfg(test)]
mod tests {
    #[test]
    fn normalized_tangents_follow_the_authored_hermite_curve() {
        // These keys describe p(t) = 2t - t², rather than the straight line t.
        let samples = super::sample_cubic_keyframes(vec![
            (0, [0.0], [0.0], [2.0]),
            (1000, [1.0], [0.0], [0.0]),
        ]);
        let actual = samples
            .iter()
            .step_by(250)
            .map(|(_, value)| value[0])
            .collect::<Vec<_>>();
        assert_eq!(actual, [0.0, 0.4375, 0.75, 0.9375, 1.0]);
    }
}
