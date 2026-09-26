//! Shared failures from renderer-neutral native source conversion.

use std::{error::Error, fmt};

#[derive(Debug)]
pub(in crate::assets) enum ConversionError {
    InvalidSource(&'static str),
    InvalidValue(&'static str),
    Json(serde_json::Error),
    Glb(gltf::Error),
}

impl fmt::Display for ConversionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidSource(message) | Self::InvalidValue(message) => {
                formatter.write_str(message)
            }
            Self::Json(error) => error.fmt(formatter),
            Self::Glb(error) => error.fmt(formatter),
        }
    }
}

impl Error for ConversionError {}

impl From<serde_json::Error> for ConversionError {
    fn from(error: serde_json::Error) -> Self {
        Self::Json(error)
    }
}

impl From<gltf::Error> for ConversionError {
    fn from(error: gltf::Error) -> Self {
        Self::Glb(error)
    }
}
