use bevy::math::{Quat, Vec3};

#[derive(Clone)]
pub(super) struct BlueFangQuaternionCurve {
    keys: Vec<(f32, Quat)>,
    controls: Option<Vec<(Quat, Quat)>>,
}

impl BlueFangQuaternionCurve {
    pub(super) fn new(mut keys: Vec<(u16, [f32; 4], [f32; 3])>, cubic: bool) -> Self {
        keys.sort_by_key(|key| key.0);
        keys.dedup_by_key(|key| key.0);
        let controls = cubic.then(|| {
            keys.iter()
                .enumerate()
                .map(|(index, current)| {
                    let previous = &keys[index.saturating_sub(1)];
                    let next = &keys[(index + 1).min(keys.len() - 1)];
                    let rotation = Quat::from_array(current.1);
                    let previous_delta =
                        quaternion_log(Quat::from_array(previous.1).inverse() * rotation);
                    let next_delta = quaternion_log(rotation.inverse() * Quat::from_array(next.1));
                    let duration = f32::from(next.0 - previous.0);
                    if duration == 0.0 {
                        return (rotation, rotation);
                    }
                    let [tension, continuity, bias] = current.2;
                    let outgoing_interval =
                        f32::from(current.0 - previous.0) / duration * (1.0 - tension);
                    let incoming_interval =
                        f32::from(next.0 - current.0) / duration * (1.0 - tension);
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
                .collect::<Vec<_>>()
        });

        Self {
            keys: keys
                .into_iter()
                .map(|(time, quaternion, _)| {
                    (f32::from(time) / 1000.0, Quat::from_array(quaternion))
                })
                .collect(),
            controls,
        }
    }

    pub(super) fn end_seconds(&self) -> Option<f32> {
        self.keys.last().map(|key| key.0)
    }

    pub(super) fn sample(&self, time: f32) -> Quat {
        let next = self.keys.partition_point(|key| key.0 <= time);
        let Some(index) = next.checked_sub(1) else {
            return self.keys.first().map_or(Quat::IDENTITY, |key| key.1);
        };
        let (left_time, left) = self.keys[index];
        let Some(&(right_time, right)) = self.keys.get(next) else {
            return left;
        };
        let fraction = (time - left_time) / (right_time - left_time);
        let linear = left.slerp(right, fraction);
        self.controls.as_ref().map_or(linear, |controls| {
            let tangent = controls[index].0.slerp(controls[next].1, fraction);
            linear
                .slerp(tangent, 2.0 * fraction * (1.0 - fraction))
                .normalize()
        })
    }
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
        let curve = super::BlueFangQuaternionCurve::new(
            vec![
                (0, Quat::IDENTITY.to_array(), [0.0; 3]),
                (
                    1000,
                    Quat::from_rotation_x(std::f32::consts::FRAC_PI_3).to_array(),
                    [0.0; 3],
                ),
            ],
            true,
        );
        let expected = Quat::from_rotation_x(9.375_f32.to_radians());
        assert!(curve.sample(0.25).abs_diff_eq(expected, 1e-5));
    }
}
