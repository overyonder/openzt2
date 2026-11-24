use bevy::prelude::*;

/// Current pose of the active zoo camera, sampled before water presentation.
#[derive(Component, Clone, Copy, Default, PartialEq)]
pub struct TerrainWaterCameraPose(pub Option<GlobalTransform>);

/// Water contact for the active zoo camera.
#[derive(Component, Debug, Default, Clone, Copy, PartialEq)]
pub struct TerrainWaterCameraContact {
    pub position: Option<Vec3>,
    pub wet_region: Option<(Entity, f32)>,
    /// None denotes unresolved membership or an inactive camera.
    pub underwater_status: Option<bool>,
}

impl TerrainWaterCameraContact {
    pub fn from_membership(position: Vec3, membership: Option<Option<(Entity, f32)>>) -> Self {
        let wet = membership.map(|region| region.filter(|(_, height)| position.y < *height));
        Self {
            position: Some(position),
            wet_region: wet.flatten(),
            underwater_status: wet.map(|region| region.is_some()),
        }
    }
}

/// The selected reflection capture and any alternate region still visible.
#[derive(Component, Debug, Default, Clone, Copy, PartialEq)]
pub struct TerrainWaterCaptureSelection {
    pub resolved: bool,
    pub alternate_regions_visible: bool,
}

/// The reflection capture's selected water region.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
#[relationship(relationship_target = TerrainWaterCaptureCameras)]
pub struct TerrainWaterCaptureRegion(pub Entity);

#[derive(Component)]
#[relationship_target(relationship = TerrainWaterCaptureRegion)]
pub struct TerrainWaterCaptureCameras(Vec<Entity>);

#[derive(Component, Debug, Default, Clone, Copy, PartialEq)]
pub struct TerrainWaterTrainingFade(pub f32);

/// Fade state belongs to the region's water effect, not a global registry.
#[derive(Component, Debug, Default, Clone, Copy, PartialEq)]
pub struct TerrainWaterRegionFade {
    fraction: f32,
    rate_per_second: f32,
}

impl TerrainWaterRegionFade {
    pub fn fraction(&self) -> f32 {
        self.fraction
    }
    pub const fn is_active(&self) -> bool {
        (self.rate_per_second > 0.0 && self.fraction < 1.0)
            || (self.rate_per_second < 0.0 && self.fraction > 0.0)
    }
    pub fn is_transitioning(&self) -> bool {
        self.fraction > 0.0 && self.fraction < 1.0
    }
    pub fn fade_in(&mut self, seconds: f32) {
        self.rate_per_second = if seconds > 0.0 { seconds.recip() } else { 1.0 };
    }
    pub fn fade_out(&mut self, seconds: f32, visible: bool) {
        self.rate_per_second = -if seconds > 0.0 { seconds.recip() } else { 1.0 };
        if !visible {
            self.fraction = 0.0;
        }
    }
    pub fn advance(&mut self, seconds: f32) {
        self.fraction = (self.fraction + self.rate_per_second * seconds).clamp(0.0, 1.0);
    }
}

/// Aggregated visibility of the region's actual rendered surface groups.
/// Region owners parent their rays and particles, so they are spatial.
#[derive(Component, Debug, Default)]
#[require(Transform, Visibility)]
pub struct TerrainWaterRegionVisibility(pub bool);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn incoming_fade_can_reverse_then_restart_without_resetting_its_fraction() {
        let mut fade = TerrainWaterRegionFade::default();
        fade.fade_in(1.5);
        fade.advance(0.375);
        assert_eq!(fade.fraction(), 0.25);
        fade.fade_out(1.5, true);
        fade.advance(0.1875);
        assert_eq!(fade.fraction(), 0.125);
        fade.fade_in(1.5);
        fade.advance(3.0);
        assert_eq!(fade.fraction(), 1.0);
        fade.fade_out(1.5, false);
        assert_eq!(fade.fraction(), 0.0);
    }

    #[test]
    fn camera_contact_preserves_dry_wet_and_unresolved_membership() {
        let wet = Entity::from_bits(1 << 32 | 2);
        let position = Vec3::new(0.0, 5.0, 0.0);
        let dry = TerrainWaterCameraContact::from_membership(position, Some(Some((wet, 4.0))));
        assert_eq!(dry.underwater_status, Some(false));
        let submerged =
            TerrainWaterCameraContact::from_membership(position, Some(Some((wet, 6.0))));
        assert_eq!(submerged.wet_region, Some((wet, 6.0)));
        assert_eq!(submerged.underwater_status, Some(true));
        let unresolved = TerrainWaterCameraContact::from_membership(position, None);
        assert_eq!(unresolved.underwater_status, None);
    }
}
