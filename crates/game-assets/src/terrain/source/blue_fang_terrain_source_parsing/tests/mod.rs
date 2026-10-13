use super::*;

#[test]
fn water_region_tank_depth_flag_uses_the_native_low_byte() {
    for (version, carrier, expected) in [
        (10, 0_u32, false),
        (11, 0, false),
        (13, 0x100, false),
        (13, 2, true),
        (14, 0, false),
        (14, 2, true),
    ] {
        let header = BlueFangTerrainSourceHeader {
            version,
            extent_x: 4.0,
            extent_y: 4.0,
            width: 5,
            height: 5,
            sector_columns: 1,
            sector_rows: 1,
        };
        let mut bytes = Vec::new();
        // One region, height 3, style 7, one member at index 12.
        for word in [1, 3.0_f32.to_bits(), 7, 1, 12] {
            bytes.extend_from_slice(&word.to_le_bytes());
        }
        if version >= 13 {
            bytes.extend_from_slice(&0.5_f32.to_le_bytes());
            let flag_bytes = carrier.to_le_bytes();
            bytes.extend_from_slice(if version == 13 {
                &flag_bytes
            } else {
                &flag_bytes[..1]
            });
        }
        bytes.extend(std::iter::repeat_n(0, if version == 10 { 1 } else { 2 }));
        let parsed = parse_blue_fang_terrain_post_grid(header, &bytes).unwrap();
        assert_eq!(parsed.water_regions[0].uses_tank_minimum_depth, expected);
        assert_eq!(parsed.water_regions[0].row_major_cell_indices, [12]);
    }
}

#[test]
fn terrain_surface_flag_is_independent_of_depth_and_starts_with_version_thirteen() {
    // Native reader layout: 27 bytes through v10, an extra flag byte in v11,
    // then the independent bit-0x40 flag byte in v13. Test literal source rows
    // rather than deriving offsets through the parser being checked.
    for (version, row_bytes, tile_bytes) in [(10_u32, 27, 1), (11, 28, 2), (13, 29, 2), (14, 29, 2)]
    {
        let mut source = vec![0_u8; 44];
        source[0..4].copy_from_slice(&version.to_le_bytes());
        source[4..8].copy_from_slice(&4.0_f32.to_le_bytes());
        source[8..12].copy_from_slice(&4.0_f32.to_le_bytes());
        source[28..32].copy_from_slice(&5_u32.to_le_bytes());
        source[32..36].copy_from_slice(&5_u32.to_le_bytes());
        source[36..40].copy_from_slice(&1_u32.to_le_bytes());
        source[40..44].copy_from_slice(&1_u32.to_le_bytes());
        source.extend_from_slice(&0_u32.to_le_bytes()); // no biome strings
        for index in 0..25 {
            let mut row = vec![0; row_bytes];
            // Native child linkage takes the low bit; linked-object presence
            // accepts every nonzero byte, independently of child linkage.
            row[17] = if index == 0 { 3 } else { 2 };
            row[26] = if index == 0 { 2 } else { 0 };
            if version >= 11 {
                row[27] = if index == 0 { 3 } else { 2 };
            } // a distinct flag must not leak
            if version >= 13 {
                row[28] = if index == 0 { 5 } else { 2 }; // native takes bit zero
            }
            source.extend_from_slice(&row);
        }
        source.extend_from_slice(&0_u32.to_le_bytes()); // no linked water regions
        source.extend(std::iter::repeat_n(0, tile_bytes));
        let terrain = parse_blue_fang_terrain_source_data(&source).unwrap();
        assert_eq!(terrain.samples[0].water_surface_flag, version >= 13);
        assert!(!terrain.samples[1].water_surface_flag);
        assert_eq!(terrain.samples[0].excludes_elevated_path, version >= 11);
        assert!(!terrain.samples[1].excludes_elevated_path);
        assert_eq!(terrain.samples[0].constraint_flag, Some(3));
        assert_eq!(terrain.samples[1].constraint_flag, Some(2));
        assert_eq!(terrain.samples[0].linked_height_shape, Some(2));
        assert_eq!(
            terrain.samples[0].water_type, 0,
            "surface flag does not synthesize shallow/deep water state"
        );
    }
}
