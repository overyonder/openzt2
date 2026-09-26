#[derive(Clone, Copy, Debug)]
pub(super) struct AuthoredPlacementFootprint {
    pub(super) minimum_xz: [f32; 2],
    pub(super) maximum_xz: [f32; 2],
}
