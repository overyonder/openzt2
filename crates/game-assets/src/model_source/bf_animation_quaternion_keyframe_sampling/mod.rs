use bevy::math::{Quat, Vec3};

pub(super) fn sample_tcb_quaternion_keyframes(
    mut keys: Vec<(u16, [f32; 4], [f32; 3])>,
) -> Vec<(u16, [f32; 4])> {
    keys.sort_by_key(|key| key.0);
    keys.dedup_by_key(|key| key.0);
    let controls = keys
        .iter()
        .enumerate()
        .map(|(index, current)| {
            let previous = &keys[index.saturating_sub(1)];
            let next = &keys[(index + 1).min(keys.len() - 1)];
            let rotation = Quat::from_array(current.1);
            let previous_delta = quaternion_log(Quat::from_array(previous.1).inverse() * rotation);
            let next_delta = quaternion_log(rotation.inverse() * Quat::from_array(next.1));
            let duration = f32::from(next.0 - previous.0);
            if duration == 0.0 {
                return (rotation, rotation);
            }
            let [tension, continuity, bias] = current.2;
            let outgoing_interval = f32::from(current.0 - previous.0) / duration * (1.0 - tension);
            let incoming_interval = f32::from(next.0 - current.0) / duration * (1.0 - tension);
            let outgoing = (previous_delta
                * ((1.0 + continuity) * (1.0 + bias) * outgoing_interval)
                + next_delta
                    * ((1.0 - continuity) * (1.0 - bias)).mul_add(outgoing_interval, -1.0))
                * 0.5;
            let incoming = (previous_delta
                * ((1.0 - continuity) * (1.0 + bias)).mul_add(-incoming_interval, 1.0)
                - next_delta * ((1.0 + continuity) * (1.0 - bias) * incoming_interval))
                * 0.5;
            (
                rotation * quaternion_exp(outgoing),
                rotation * quaternion_exp(incoming),
            )
        })
        .collect::<Vec<_>>();
    let mut samples = Vec::new();
    for (index, pair) in keys.windows(2).enumerate() {
        let left = Quat::from_array(pair[0].1);
        let right = Quat::from_array(pair[1].1);
        for time in pair[0].0..pair[1].0 {
            let t = f32::from(time - pair[0].0) / f32::from(pair[1].0 - pair[0].0);
            let linear = left.slerp(right, t);
            let tangent = controls[index].0.slerp(controls[index + 1].1, t);
            samples.push((
                time,
                linear
                    .slerp(tangent, 2.0 * t * (1.0 - t))
                    .normalize()
                    .to_array(),
            ));
        }
    }
    if let Some(last) = keys.last() {
        samples.push((last.0, last.1));
    }
    samples
}

fn quaternion_log(rotation: Quat) -> Vec3 {
    let angle = rotation.w.clamp(-1.0, 1.0).acos();
    let sine = angle.sin();
    Vec3::new(rotation.x, rotation.y, rotation.z)
        * if sine.abs() > 0.0001 {
            angle / sine
        } else {
            1.0
        }
}

fn quaternion_exp(value: Vec3) -> Quat {
    let angle = value.length();
    let vector = value
        * if angle > 0.0001 {
            angle.sin() / angle
        } else {
            1.0
        };
    Quat::from_xyzw(vector.x, vector.y, vector.z, angle.cos()).normalize()
}

#[cfg(test)]
mod tests {
    #[test]
    fn two_rotation_keys_have_zero_endpoint_tangents() {
        use bevy::math::Quat;
        // With duplicated boundary keys and zero TCB parameters, a 60° turn
        // follows 60° * (3t² - 2t³): 9.375° at a quarter of the interval.
        let samples = super::sample_tcb_quaternion_keyframes(vec![
            (0, Quat::IDENTITY.to_array(), [0.0; 3]),
            (
                1000,
                Quat::from_rotation_x(std::f32::consts::FRAC_PI_3).to_array(),
                [0.0; 3],
            ),
        ]);
        let expected = Quat::from_rotation_x(9.375_f32.to_radians());
        assert!(Quat::from_array(samples[250].1).abs_diff_eq(expected, 1e-5));
    }
}
