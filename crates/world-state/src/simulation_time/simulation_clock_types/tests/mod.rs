use super::{ZooCalendar, ZooMonth};

#[test]
fn the_first_month_of_a_zoo_is_month_zero() {
    let month = |year, month| {
        ZooMonth::from(ZooCalendar {
            year,
            month,
            day: 1,
        })
        .0
    };
    assert_eq!(month(1, 1), 0);
    assert_eq!(month(1, 12), 11);
    assert_eq!(month(2, 1), 12);
}
