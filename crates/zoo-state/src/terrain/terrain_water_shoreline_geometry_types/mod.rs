use bevy::prelude::{IVec2, Vec2};

#[derive(Clone, Debug, Default)]
pub struct TerrainWaterStitchGeometry {
    pub positions: Vec<[f32; 3]>,
    pub indices: Vec<u32>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TerrainShorelineBoundaryEdge {
    pub land: IVec2,
    pub water: IVec2,
}

#[derive(Clone, Debug)]
pub struct TerrainShorelineBoundary {
    pub edges: Box<[TerrainShorelineBoundaryEdge]>,
    pub closed: bool,
}

/// Derived geometry for one five-by-five shoreline patch. The third position
/// coordinate is the horizontal wave displacement weight, not world height.
#[derive(Clone, Debug)]
pub struct TerrainShorelinePatch {
    pub positions: [[f32; 3]; 25],
    pub texture_coordinates: [[f32; 2]; 25],
    /// Source side order: north, east, south, west.
    pub included_sides: [bool; 4],
    pub selector_codes: [u8; 4],
}

/// One point of a closed shoreline profile, in world horizontal coordinates.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TerrainShorelineProfilePoint {
    pub position: Vec2,
    pub longitudinal_coordinate: f32,
}

/// A closed profile with normalized longitudinal coordinates and its shore width.
#[derive(Clone, Debug)]
pub struct TerrainShorelineProfile {
    points: Box<[TerrainShorelineProfilePoint]>,
    distance_scale: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TerrainShorelineProfileError {
    Empty,
    InvalidDistanceScale,
    InvalidPoint,
    ZeroLength,
}

impl TerrainShorelineProfile {
    /// # Errors
    /// Rejects empty profiles, invalid point coordinates and a
    /// nonpositive or nonfinite distance scale.
    pub fn from_normalized_points(
        points: Box<[TerrainShorelineProfilePoint]>,
        distance_scale: f32,
    ) -> Result<Self, TerrainShorelineProfileError> {
        if points.is_empty() {
            return Err(TerrainShorelineProfileError::Empty);
        }
        if !distance_scale.is_finite() || distance_scale <= 0.0 {
            return Err(TerrainShorelineProfileError::InvalidDistanceScale);
        }
        if points.iter().any(|point| {
            !point.position.is_finite()
                || !point.longitudinal_coordinate.is_finite()
                || point.longitudinal_coordinate < 0.0
        }) {
            return Err(TerrainShorelineProfileError::InvalidPoint);
        }
        Ok(Self {
            points,
            distance_scale,
        })
    }

    pub fn points(&self) -> &[TerrainShorelineProfilePoint] {
        &self.points
    }

    pub fn distance_scale(&self) -> f32 {
        self.distance_scale
    }
}
