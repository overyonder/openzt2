#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct StaffRequestRank {
    pub(super) priority: f32,
    pub(super) age: i32,
    pub(super) distance: f32,
}

impl StaffRequestRank {
    pub(super) const INITIAL: Self = Self {
        priority: f32::MAX,
        age: -1000,
        distance: 1_000_000.0,
    };

    pub(super) fn outranks(self, previous: Self) -> bool {
        self.priority < previous.priority
            || (self.priority == previous.priority
                && (self.age > previous.age
                    || (self.age == previous.age && self.distance < previous.distance)))
    }
}

pub(super) fn request_age(now: f64, issued_at: f64, category_duration: i32) -> i32 {
    if category_duration < 1 {
        0
    } else {
        (now - issued_at) as i32 / category_duration
    }
}

pub(super) fn can_steal_request(
    distance: f32,
    incumbent_distance: f32,
    safe_distance: f32,
    threshold_percent: f32,
) -> bool {
    distance < incumbent_distance
        && incumbent_distance > safe_distance
        && (incumbent_distance - distance) / incumbent_distance >= threshold_percent / 100.0
}

pub(super) fn staff_search_retry_deadline(now: f64, draw: u16) -> f64 {
    now + 4.0 + 2.0 * (f64::from(draw) / 32_767.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn staff_requests_compare_priority_then_signed_age_then_planar_distance() {
        let older = StaffRequestRank {
            priority: 6.0,
            age: request_age(600.0, 299.0, 300),
            distance: 10.0,
        };
        let newer = StaffRequestRank {
            priority: 6.0,
            age: request_age(600.0, 301.0, 300),
            distance: 1.0,
        };
        let urgent = StaffRequestRank {
            priority: 5.0,
            ..newer
        };
        assert_eq!((older.age, newer.age), (1, 0));
        assert!(older.outranks(newer));
        assert!(urgent.outranks(older));
        assert!(!urgent.outranks(urgent));
    }

    #[test]
    fn theft_includes_the_improvement_boundary_and_excludes_safe_distance() {
        assert!(can_steal_request(9.0, 10.0, 6.0, 10.0));
        assert!(!can_steal_request(0.0, 6.0, 6.0, 10.0));
    }

    #[test]
    fn failed_search_at_20_01_can_defer_until_26_01() {
        assert!((staff_search_retry_deadline(20.01, 32_767) - 26.01).abs() < f64::EPSILON);
    }
}
