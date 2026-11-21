use super::{
    maintenance_types::{
        FacilityContainedWaste, FacilityContainedWastePresentationLevel, LooseLitterWaste,
    },
    zoo_cleanliness_projection::{
        aggregate_changed_waste_into_zoo_cleanliness_totals,
        calculate_weighted_zoo_cleanliness_permille_from_explicit_weights,
        divide_unsigned_ratio_with_half_up_rounding, ZooCleanlinessTotals,
    },
};
use bevy::prelude::*;
use openzt2_world_state::world_spawn::world_membership_types::{WorldMember, WorldRoot};

#[test]
fn trash_level_is_derived_from_authoritative_container_thirds() {
    let presentation_level_for_contained_waste_units = |contained_waste_units| {
        FacilityContainedWastePresentationLevel::calculate_from_facility_contained_waste(
            FacilityContainedWaste {
                contained_waste_units,
                contained_waste_capacity_units: 9,
            },
        )
    };
    assert_eq!(
        presentation_level_for_contained_waste_units(0),
        FacilityContainedWastePresentationLevel::Empty
    );
    assert_eq!(
        presentation_level_for_contained_waste_units(1),
        FacilityContainedWastePresentationLevel::OneThird
    );
    assert_eq!(
        presentation_level_for_contained_waste_units(3),
        FacilityContainedWastePresentationLevel::OneThird
    );
    assert_eq!(
        presentation_level_for_contained_waste_units(4),
        FacilityContainedWastePresentationLevel::TwoThirds
    );
    assert_eq!(
        presentation_level_for_contained_waste_units(6),
        FacilityContainedWastePresentationLevel::TwoThirds
    );
    assert_eq!(
        presentation_level_for_contained_waste_units(7),
        FacilityContainedWastePresentationLevel::Full
    );
}

#[test]
fn cleanliness_reduction_tracks_spawn_and_despawn() {
    let mut app = App::new();
    app.register_required_components::<WorldRoot, ZooCleanlinessTotals>()
        .add_systems(Update, aggregate_changed_waste_into_zoo_cleanliness_totals);
    let root = app
        .world_mut()
        .spawn(WorldRoot {
            scenario: Default::default(),
        })
        .id();
    let entity = app
        .world_mut()
        .spawn((
            WorldMember { root },
            FacilityContainedWaste {
                contained_waste_units: 2,
                contained_waste_capacity_units: 8,
            },
            LooseLitterWaste {
                uncontained_waste_units: 3,
            },
        ))
        .id();
    app.update();
    let totals = *app
        .world()
        .get::<ZooCleanlinessTotals>(root)
        .expect("world cleanliness totals");
    assert_eq!(totals.contained_waste_units, 2);
    assert_eq!(totals.contained_waste_capacity_units, 8);
    assert_eq!(totals.uncontained_waste_units, 3);

    app.world_mut().despawn(entity);
    app.update();
    assert_eq!(
        *app.world()
            .get::<ZooCleanlinessTotals>(root)
            .expect("world cleanliness totals"),
        ZooCleanlinessTotals::default()
    );
}

#[test]
fn cleanliness_uses_nearest_integer_weighting() {
    let totals = ZooCleanlinessTotals {
        contained_waste_units: 1,
        contained_waste_capacity_units: 3,
        uncontained_waste_units: 1,
    };
    assert_eq!(divide_unsigned_ratio_with_half_up_rounding(1_501, 2), 751);
    assert_eq!(
        calculate_weighted_zoo_cleanliness_permille_from_explicit_weights(totals, 1, 1, 4),
        709
    );
    assert_eq!(
        calculate_weighted_zoo_cleanliness_permille_from_explicit_weights(
            ZooCleanlinessTotals::default(),
            1,
            1,
            10,
        ),
        1_000
    );
}
