//! FX2 compilation and evaluation through vkd3d-shader and MojoShader.
//!
//! Inline shader assembly is assembled with MojoShader before FX2 compilation.

pub mod effect_compilation;
pub mod effect_evaluation;
pub mod effect_types;
pub mod error;
mod inline_assembly;
mod native_abi;
pub mod shader_translation;
pub mod shader_types;
mod spirv_separate_sampler_bindings;
#[cfg(test)]
mod tests;
