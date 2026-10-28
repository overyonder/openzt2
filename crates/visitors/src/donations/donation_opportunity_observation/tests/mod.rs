use super::*;

#[test]
fn signed_view_scores_and_favourite_factor_fund_one_capped_donation() {
    // Guest threshold 70, preferred factor 2, lion base $80 and species cap 1.
    let mut meter = 0.0;
    assert_eq!(
        advance_species_donation_meter(&mut meter, 30.0, 2.0, 70.0, 1.0, Money(8000)),
        None
    );
    assert_eq!(meter, 60.0);
    assert_eq!(
        advance_species_donation_meter(&mut meter, -50.0, 2.0, 70.0, 1.0, Money(8000)),
        None
    );
    assert_eq!(meter, -40.0);
    assert_eq!(
        advance_species_donation_meter(&mut meter, 60.0, 2.0, 70.0, 1.0, Money(8000)),
        Some(Money(8000))
    );
    assert_eq!(meter, 10.0);
}
