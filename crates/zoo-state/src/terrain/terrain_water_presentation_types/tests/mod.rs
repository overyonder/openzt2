use super::*;

#[test]
fn selector_patches_preserve_resolved_fallback_and_untouched_tiles() {
    let mut membership = TerrainWaterMembershipIndex::new(
        Handle::<TerrainAsset>::default().id(),
        9,
        5,
        vec![None; 45].into_boxed_slice(),
        Box::new([[None; 4]; 2]),
        false,
    )
    .unwrap();
    let empty_probes = || std::array::from_fn(|_| Box::default());
    assert_eq!(membership.selector_codes(UVec2::X), None);
    membership
        .patch_selector_geometry(vec![(UVec2::X, [1, 0, 0, 0], empty_probes())], false)
        .unwrap();
    assert_eq!(membership.selector_codes(UVec2::ZERO), Some(&[0; 4]));
    assert_eq!(membership.selector_codes(UVec2::X), Some(&[1, 0, 0, 0]));
    membership
        .patch_selector_geometry(vec![(UVec2::X, [0; 4], empty_probes())], true)
        .unwrap();
    assert_eq!(membership.selector_codes(UVec2::X), Some(&[0; 4]));
    assert!(membership.selectors.as_ref().unwrap().is_empty());
}

#[test]
fn replacing_or_clearing_wet_sides_tracks_the_last_wet_tile() {
    let mut world = World::new();
    let owner = world.spawn_empty().id();
    let wet = [Some(owner), None, None, None];
    let dry = [None; 4];
    let mut membership = TerrainWaterMembershipIndex::new(
        Handle::<TerrainAsset>::default().id(),
        9,
        5,
        vec![None; 45].into_boxed_slice(),
        Box::new([wet, dry]),
        true,
    )
    .unwrap();
    assert!(membership.has_water_sides());
    assert_eq!(membership.set_side_owners(UVec2::ZERO, wet), Some(false));
    assert_eq!(membership.set_side_owners(UVec2::X, wet), Some(true));
    assert_eq!(membership.set_side_owners(UVec2::ZERO, dry), Some(true));
    assert!(membership.has_water_sides());
    assert_eq!(membership.set_side_owners(UVec2::X, dry), Some(true));
    assert!(!membership.has_water_sides());
}
