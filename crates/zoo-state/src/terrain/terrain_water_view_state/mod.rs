use bevy::prelude::*;

/// Camera-dependent water presentation; region entities remain authoritative.
#[derive(Resource, Default)]
pub struct TerrainWaterViewState {
    camera_position: Option<Vec3>,
    camera_region: Option<Entity>,
    camera_surface_height: Option<f32>,
    underwater_status: Option<bool>,
    selected_region: Option<Entity>,
    surface_height: Option<f32>,
    selection_resolved: bool,
    alternate_regions_visible: bool,
}

impl TerrainWaterViewState {
    pub fn camera_position(&self) -> Option<Vec3> {
        self.camera_position
    }
    pub fn camera_region(&self) -> Option<Entity> {
        self.camera_region
    }
    pub fn camera_surface_height(&self) -> Option<f32> {
        self.camera_surface_height
    }
    /// None denotes unresolved membership or no active camera.
    pub fn underwater_status(&self) -> Option<bool> {
        self.underwater_status
    }
    pub fn selected_region(&self) -> Option<Entity> {
        self.selected_region
    }
    pub fn surface_height(&self) -> Option<f32> {
        self.surface_height
    }
    pub fn selection_resolved(&self) -> bool {
        self.selection_resolved
    }
    pub fn alternate_regions_visible(&self) -> bool {
        self.alternate_regions_visible
    }

    pub fn update_camera(&mut self, position: Option<Vec3>, region: Option<Option<(Entity, f32)>>) {
        self.camera_position = position;
        let region_above_camera = position
            .zip(region)
            .map(|(position, region)| region.filter(|(_, height)| position.y < *height));
        self.underwater_status = region_above_camera.map(|region| region.is_some());
        self.camera_region = region_above_camera.flatten().map(|(entity, _)| entity);
        self.camera_surface_height = region_above_camera.flatten().map(|(_, height)| height);
        if position.is_none() {
            *self = Self::default();
        }
    }

    pub fn update_selection(
        &mut self,
        region: Option<Option<(Entity, f32)>>,
        alternate_visible: bool,
    ) {
        self.selection_resolved = region.is_some();
        self.selected_region = region.flatten().map(|(entity, _)| entity);
        self.surface_height = region.flatten().map(|(_, height)| height);
        self.alternate_regions_visible = alternate_visible;
    }
}

/// Fade state belongs to the region's water effect, not a global registry.
#[derive(Component, Debug, Default)]
pub struct TerrainWaterRegionFade {
    fraction: f32,
    rate_per_second: f32,
}

impl TerrainWaterRegionFade {
    pub fn fraction(&self) -> f32 {
        self.fraction
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
#[derive(Component, Debug, Default)]
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
    fn camera_wet_region_is_independent_of_the_capture_region() {
        let capture = Entity::from_bits(1 << 32 | 1);
        let wet = Entity::from_bits(1 << 32 | 2);
        let mut view = TerrainWaterViewState::default();
        view.update_selection(Some(Some((capture, 20.0))), false);
        view.update_camera(Some(Vec3::new(0.0, 5.0, 0.0)), Some(None));
        assert_eq!(view.underwater_status(), Some(false));
        assert_eq!(view.selected_region(), Some(capture));
        view.update_camera(Some(Vec3::new(0.0, 5.0, 0.0)), Some(Some((wet, 6.0))));
        assert_eq!(view.camera_region(), Some(wet));
        assert_eq!(view.underwater_status(), Some(true));
        view.update_camera(Some(Vec3::new(0.0, 5.0, 0.0)), None);
        assert_eq!(view.underwater_status(), None);
    }
}
