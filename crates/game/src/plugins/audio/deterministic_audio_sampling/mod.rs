pub(super) fn sample_deterministic_f32_range(range: [f32; 2], selection: u64) -> f32 {
    let unit_interval_sample = (selection.rotate_left(29) >> 40) as f32 / (1_u64 << 24) as f32;
    range[0] + (range[1] - range[0]) * unit_interval_sample
}
