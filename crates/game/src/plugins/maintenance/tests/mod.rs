use super::{
    cleanliness_aggregation_operations::{
        calculate_weighted_zoo_cleanliness_permille_from_explicit_weights,
        divide_unsigned_ratio_with_half_up_rounding,
    },
    maintenance_types::{
        FacilityContainedWaste, FacilityContainedWastePresentationLevel,
        IncrementalZooCleanlinessTotals, LooseLitterWaste, MaintainableObjectConditionPermille,
    },
    MaintenancePlugin,
};
use bevy::prelude::*;

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
fn component_hooks_track_spawn_and_despawn_without_an_entity_registry() {
    let mut app = App::new();
    app.add_plugins(MaintenancePlugin);
    let entity = app
        .world_mut()
        .spawn((
            MaintainableObjectConditionPermille(600),
            FacilityContainedWaste {
                contained_waste_units: 2,
                contained_waste_capacity_units: 8,
            },
            LooseLitterWaste {
                uncontained_waste_units: 3,
            },
        ))
        .id();
    let totals = *app.world().resource::<IncrementalZooCleanlinessTotals>();
    assert_eq!(totals.maintainable_condition_permille_sum, 600);
    assert_eq!(totals.maintainable_object_count, 1);
    assert_eq!(totals.contained_waste_units, 2);
    assert_eq!(totals.contained_waste_capacity_units, 8);
    assert_eq!(totals.uncontained_waste_units, 3);

    app.world_mut().despawn(entity);
    assert_eq!(
        *app.world().resource::<IncrementalZooCleanlinessTotals>(),
        IncrementalZooCleanlinessTotals {
            requires_recalculation: true,
            ..Default::default()
        }
    );
}

#[test]
fn cleanliness_uses_nearest_integer_weighting() {
    let totals = IncrementalZooCleanlinessTotals {
        maintainable_condition_permille_sum: 1_501,
        maintainable_object_count: 2,
        contained_waste_units: 1,
        contained_waste_capacity_units: 3,
        uncontained_waste_units: 1,
        requires_recalculation: true,
    };
    assert_eq!(divide_unsigned_ratio_with_half_up_rounding(1_501, 2), 751);
    assert_eq!(
        calculate_weighted_zoo_cleanliness_permille_from_explicit_weights(totals, 2, 1, 1, 4,),
        730
    );
    assert_eq!(
        calculate_weighted_zoo_cleanliness_permille_from_explicit_weights(
            IncrementalZooCleanlinessTotals::default(),
            1,
            1,
            1,
            10,
        ),
        1_000
    );
}
