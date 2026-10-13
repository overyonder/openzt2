#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NativeGeometrySourceFamily {
    Bfb,
    Nif,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NativeGeometryLoweringErrorKind {
    MissingMaterial { reference: String },
    MissingGeometry { block: u32 },
    InvalidMaterial { detail: String },
    InvalidTopology { detail: &'static str },
    InvalidVertexData { detail: &'static str },
    UnsupportedPrimitive { primitive: u8 },
    SkeletonLayoutMissing { geometry: String },
}

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[error("{family:?} source {path}: {kind:?}")]
pub struct NativeGeometryLoweringError {
    pub family: NativeGeometrySourceFamily,
    pub path: String,
    pub kind: NativeGeometryLoweringErrorKind,
}

impl NativeGeometryLoweringError {
    pub fn new(
        family: NativeGeometrySourceFamily,
        path: impl Into<String>,
        kind: NativeGeometryLoweringErrorKind,
    ) -> Self {
        Self {
            family,
            path: path.into(),
            kind,
        }
    }
}
