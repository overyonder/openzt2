use super::*;
use openzt2_game_data::world_definitions::world_objects::{
    WorldObjectTextureReplacementGroup, WorldObjectTextureReplacementSet,
};
use openzt2_game_data::AssetId;

fn variant(weights: &[f32]) -> StaffPresentationVariant {
    StaffPresentationVariant {
        prefab: AssetId::default(),
        model_animation_set: AssetId::default(),
        name_pool: AssetId::default(),
        head: None,
        texture_replacement_sets: vec![WorldObjectTextureReplacementSet {
            groups: weights
                .iter()
                .map(|weight| WorldObjectTextureReplacementGroup {
                    weight: *weight,
                    items: Vec::new(),
                })
                .collect(),
        }],
    }
}

#[test]
fn sequential_employees_spread_across_variants_and_weighted_groups() {
    let mut variant_counts = [0_u32; 4];
    let looks = variant(&[1.0, 1.0, 0.1]);
    let mut group_counts = [0_u32; 3];
    for persistent_id in 0..4000 {
        variant_counts[(mix(persistent_id) % 4) as usize] += 1;
        for group in drawn_texture_replacement_groups(&looks, persistent_id) {
            group_counts[group] += 1;
        }
    }
    assert!(variant_counts
        .iter()
        .all(|count| (800..1200).contains(count)));
    assert!(group_counts[0] > 1500 && group_counts[1] > 1500);
    assert!((100..300).contains(&group_counts[2]), "{group_counts:?}");
    // The draw is a pure function of the persistent id.
    assert!(drawn_texture_replacement_groups(&looks, 42)
        .eq(drawn_texture_replacement_groups(&looks, 42)));
}
