//! Particle-effect data shared by loaders and runtime systems.

use serde::{Deserialize, Serialize};

use self::{
    authored_particle_system::AuthoredParticleSystemDocument,
    legacy_nif_particle_effect::LegacyNifParticleEffect,
};

pub mod authored_particle_system;
pub mod legacy_nif_particle_effect;

/// The one particle-effect asset shape accepted by Bevy for embedded NIF
/// particle objects and standalone authored PSYS documents.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(untagged)]
pub enum ParticleEffectDocument {
    Effect(AuthoredParticleSystemDocument),
    LegacyNif(LegacyNifParticleEffect),
}
