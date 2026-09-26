//! BFB and NIF geometry conversion.

pub(super) mod bfb_geometry_and_skin_lowering;
pub(super) mod nif_geometry_lowering;

mod derived_normal_and_tangent_calculation;
pub(super) mod native_geometry_identity;
mod nif_material_lowering;
mod nif_skin_lowering;
pub(crate) mod source_transform_conversion;
mod validated_vertex_topology_assembly;
