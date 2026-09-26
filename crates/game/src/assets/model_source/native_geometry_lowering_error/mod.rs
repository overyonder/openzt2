#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum NativeGeometrySourceFamily {
    Bfb,
    Nif,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum NativeGeometryLoweringErrorKind {
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
pub(crate) struct NativeGeometryLoweringError {
    pub(crate) family: NativeGeometrySourceFamily,
    pub(crate) path: String,
    pub(crate) kind: NativeGeometryLoweringErrorKind,
}

impl NativeGeometryLoweringError {
    pub(crate) fn new(
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
