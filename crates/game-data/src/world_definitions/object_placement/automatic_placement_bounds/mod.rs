use super::{FootprintCell, FootprintCellFlags, PlaceableDefinition};

impl PlaceableDefinition {
    /// Derives the footprint from model bounds. Returns false if the cell
    /// dimensions or centimetre offsets exceed their field ranges.
    pub fn apply_source_lowered_automatic_placement_bounds(
        &mut self,
        [minimum_xz, maximum_xz]: [[f32; 2]; 2],
    ) -> bool {
        let dimensions = [
            (maximum_xz[0] - minimum_xz[0]).ceil().max(1.0),
            (maximum_xz[1] - minimum_xz[1]).ceil().max(1.0),
        ];
        let pivot_cm = minimum_xz.map(|minimum| -minimum * 100.0);
        if dimensions
            .iter()
            .any(|value| !value.is_finite() || !(1.0..=f32::from(i16::MAX)).contains(value))
            || pivot_cm.iter().any(|value| {
                !value.is_finite() || *value < f32::from(i16::MIN) || *value > f32::from(i16::MAX)
            })
        {
            return false;
        }
        #[allow(
            clippy::cast_possible_truncation,
            reason = "ceil produced integers and the bounds check above proves they fit i16"
        )]
        let dimensions = dimensions.map(|value| value as i16);
        #[allow(
            clippy::cast_possible_truncation,
            reason = "the bounds check above proves these rounded centimetre values fit i16"
        )]
        let pivot_cm = pivot_cm.map(|value| value.round() as i16);
        let footprint = (0..dimensions[1])
            .flat_map(|z| {
                (0..dimensions[0]).map(move |x| FootprintCell {
                    offset: [x, z],
                    flags: FootprintCellFlags::OCCUPIED,
                })
            })
            .collect::<Vec<_>>();
        self.footprint.clone_from(&footprint);
        self.pivot_cm = pivot_cm;
        self.diagonal_footprint = footprint;
        self.diagonal_pivot_cm = pivot_cm;
        true
    }
}
