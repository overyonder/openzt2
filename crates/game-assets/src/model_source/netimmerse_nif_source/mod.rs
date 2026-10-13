//! NetImmerse NIF parsing.

pub mod animation_controller_source_parsing;
pub mod animation_controller_source_types;
pub mod block_payload;
pub mod block_payload_parsing;
pub mod collision_and_skin_source_parsing;
pub mod collision_and_skin_source_reading;
pub mod collision_source_types;
pub mod counted_source_collection_reading;
pub mod document_source_parsing_and_queries;
pub mod document_source_types;
pub mod geometry_data_source_parsing;
pub mod geometry_data_source_types;
pub mod interpolated_key_data_source_parsing;
pub mod interpolated_key_source_reading;
pub mod interpolated_key_source_types;
pub mod particle_source_parsing;
pub mod particle_source_types;
pub mod render_property_and_texture_source_parsing;
pub mod render_property_and_texture_source_types;
pub mod render_texture_source_reading;
pub mod scene_object_source_parsing;
pub mod scene_object_source_types;
pub mod source_error;
pub mod transform_and_skin_source_types;

pub(super) const NETIMMERSE_VERSION_10_0_1_3: u32 = 0x0a00_0103;
pub(super) const NETIMMERSE_VERSION_10_1_0_0: u32 = 0x0a01_0000;
pub(super) const NETIMMERSE_VERSION_10_2_0_0: u32 = 0x0a02_0000;
