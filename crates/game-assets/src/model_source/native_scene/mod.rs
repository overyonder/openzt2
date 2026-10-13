//! Scene prefab conversion for native model formats.

pub mod automatic_placement_bounds_lowering;
pub mod bfb_scene_prefab_lowering;
pub mod nif_scene_prefab_lowering;

pub mod native_scene_lowering_error;
pub mod nif_collider_lowering;
pub mod nif_object_transform_controller_lowering;
pub mod scene_prefab_document_assembly;
pub mod scene_transform_and_collider_math;
