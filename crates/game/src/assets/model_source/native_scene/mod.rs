//! Scene prefab conversion for native model formats.

pub(super) mod automatic_placement_bounds_lowering;
pub(super) mod bfb_scene_prefab_lowering;
pub(super) mod nif_scene_prefab_lowering;

pub(super) mod native_scene_lowering_error;
mod nif_collider_lowering;
mod nif_object_transform_controller_lowering;
mod scene_prefab_document_assembly;
mod scene_transform_and_collider_math;
