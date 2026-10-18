use bevy::prelude::*;

use openzt2_zoo_state::terrain::terrain_chunk_types::EditedTerrainSamples;
use openzt2_zoo_state::terrain::terrain_edit_data_types::TerrainSample;
use openzt2_zoo_state::terrain::terrain_edit_rectangle_operations::apply_terrain_sample_rectangle;

#[test]
fn terrain_sample_rectangle_before_and_after_round_trip_is_exact() {
    let terrain_chunk_side = 4_u16;
    let terrain_sample_count = terrain_chunk_side as usize * terrain_chunk_side as usize;
    let mut edited_terrain_samples = EditedTerrainSamples {
        heights_cm: (0..terrain_sample_count)
            .map(|value| value as i16)
            .collect::<Vec<_>>()
            .into_boxed_slice(),
        blend_weights: vec![1; terrain_sample_count * 16].into_boxed_slice(),
        ground_cover: vec![0; terrain_sample_count].into_boxed_slice(),
        water_styles: vec![0; terrain_sample_count].into_boxed_slice(),
        water_biome_channels: vec![0; terrain_sample_count].into_boxed_slice(),
        water_cm: vec![0; terrain_sample_count].into_boxed_slice(),
        water_affiliations: vec![None; terrain_sample_count].into_boxed_slice(),
    };
    let terrain_samples_before_change = [
        terrain_sample_with_uniform_surface_values(5, 1, 0, 0, 0),
        terrain_sample_with_uniform_surface_values(6, 1, 0, 0, 0),
        terrain_sample_with_uniform_surface_values(9, 1, 0, 0, 0),
        terrain_sample_with_uniform_surface_values(10, 1, 0, 0, 0),
    ];
    let terrain_samples_after_change = [
        terrain_sample_with_uniform_surface_values(105, 3, 1, 1, 120),
        terrain_sample_with_uniform_surface_values(106, 3, 1, 1, 120),
        terrain_sample_with_uniform_surface_values(109, 3, 1, 1, 120),
        terrain_sample_with_uniform_surface_values(110, 3, 1, 1, 120),
    ];
    let minimum_terrain_sample = UVec2::new(1, 1);
    let maximum_terrain_sample = UVec2::new(2, 2);
    apply_terrain_sample_rectangle(
        &mut edited_terrain_samples,
        terrain_chunk_side,
        minimum_terrain_sample,
        maximum_terrain_sample,
        &terrain_samples_after_change,
    );
    assert_eq!(edited_terrain_samples.water_biome_channels[5], 1);
    assert_eq!(
        edited_terrain_samples.water_affiliations[5],
        Some(
            openzt2_zoo_state::terrain::terrain_edit_data_types::TerrainWaterAffiliation::SourceRow(
                7
            )
        )
    );
    apply_terrain_sample_rectangle(
        &mut edited_terrain_samples,
        terrain_chunk_side,
        minimum_terrain_sample,
        maximum_terrain_sample,
        &terrain_samples_before_change,
    );
    assert_eq!(edited_terrain_samples.heights_cm[5], 5);
    assert_eq!(edited_terrain_samples.heights_cm[6], 6);
    assert_eq!(edited_terrain_samples.heights_cm[9], 9);
    assert_eq!(edited_terrain_samples.heights_cm[10], 10);
    assert_eq!(edited_terrain_samples.water_biome_channels[5], 0);
    assert_eq!(edited_terrain_samples.water_affiliations[5], None);
    apply_terrain_sample_rectangle(
        &mut edited_terrain_samples,
        terrain_chunk_side,
        minimum_terrain_sample,
        maximum_terrain_sample,
        &terrain_samples_after_change,
    );
    assert_eq!(edited_terrain_samples.water_biome_channels[5], 1);
}

fn terrain_sample_with_uniform_surface_values(
    height_cm: i16,
    blend_weight: u8,
    ground_cover: u8,
    water_style: u16,
    water_cm: i16,
) -> TerrainSample {
    TerrainSample {
        height_cm,
        blend: [blend_weight; 16],
        ground_cover,
        water_style,
        water_biome_channel: ground_cover,
        water_cm,
        water_affiliation: (water_style != 0).then_some(
            openzt2_zoo_state::terrain::terrain_edit_data_types::TerrainWaterAffiliation::SourceRow(
                7,
            ),
        ),
    }
}

#[test]
fn region_merge_undo_restores_the_same_source_entity_and_override() {
    use crate::terrain::terrain_edit_application::apply_water_region_edit_changes;
    use openzt2_zoo_state::terrain::terrain_water_presentation_types::{
        TerrainWaterRegionEdit, TerrainWaterRegionRetired, TerrainWaterRegionSource,
    };
    let mut world = World::new();
    let terrain = Handle::default();
    let owner = world
        .spawn(TerrainWaterRegionSource::new(terrain.clone(), 7))
        .id();
    let original = TerrainWaterRegionEdit {
        uses_tank_minimum_depth: false,
        terrain,
        biome_index: 2,
        surface_height_metres: 4.0,
        depth_ratio: 0.25,
    };
    apply_water_region_edit_changes(
        &mut world.commands(),
        vec![(owner, Some(original.clone()), true)].into_boxed_slice(),
    );
    world.flush();
    apply_water_region_edit_changes(
        &mut world.commands(),
        vec![(owner, None, false)].into_boxed_slice(),
    );
    world.flush();
    assert!(world.get::<TerrainWaterRegionRetired>(owner).is_some());
    assert!(world.get::<TerrainWaterRegionEdit>(owner).is_none());
    apply_water_region_edit_changes(
        &mut world.commands(),
        vec![(owner, Some(original.clone()), true)].into_boxed_slice(),
    );
    world.flush();
    assert!(world.get::<TerrainWaterRegionRetired>(owner).is_none());
    assert_eq!(world.get::<TerrainWaterRegionEdit>(owner), Some(&original));
    assert_eq!(
        world
            .get::<TerrainWaterRegionSource>(owner)
            .expect("source entity survives undo")
            .source_region_index(),
        7
    );
}
