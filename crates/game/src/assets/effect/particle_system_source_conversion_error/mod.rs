//! Diagnostics produced while lowering an authored PSYS document.

use std::{error::Error, fmt};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct ParticleSystemSourceConversionError {
    particle_system_asset_path: String,
    source_node_description: String,
    conversion_failure: String,
}

impl fmt::Display for ParticleSystemSourceConversionError {
    fn fmt(&self, output: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            output,
            "{} ({}): {}",
            self.particle_system_asset_path, self.source_node_description, self.conversion_failure,
        )
    }
}

impl Error for ParticleSystemSourceConversionError {}

pub(super) fn particle_system_source_conversion_failure(
    particle_system_asset_path: impl Into<String>,
    source_node_description: impl Into<String>,
    conversion_failure: impl Into<String>,
) -> ParticleSystemSourceConversionError {
    ParticleSystemSourceConversionError {
        particle_system_asset_path: particle_system_asset_path.into(),
        source_node_description: source_node_description.into(),
        conversion_failure: conversion_failure.into(),
    }
}
