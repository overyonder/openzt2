//! NetImmerse NIF parsing.

pub(super) mod animation_controller_source_parsing;
pub(super) mod animation_controller_source_types;
pub(super) mod block_payload;
pub(super) mod block_payload_parsing;
pub(super) mod collision_and_skin_source_parsing;
pub(super) mod collision_and_skin_source_reading;
pub(super) mod collision_source_types;
pub(super) mod counted_source_collection_reading;
pub(super) mod document_source_parsing_and_queries;
pub(super) mod document_source_types;
pub(super) mod geometry_data_source_parsing;
pub(super) mod geometry_data_source_types;
pub(super) mod interpolated_key_data_source_parsing;
pub(super) mod interpolated_key_source_reading;
pub(super) mod interpolated_key_source_types;
pub(super) mod particle_source_parsing;
pub(super) mod particle_source_types;
pub(super) mod render_property_and_texture_source_parsing;
pub(super) mod render_property_and_texture_source_types;
pub(super) mod render_texture_source_reading;
pub(super) mod scene_object_source_parsing;
pub(super) mod scene_object_source_types;
pub(super) mod source_error;
pub(super) mod transform_and_skin_source_types;

pub(super) const NETIMMERSE_VERSION_10_0_1_3: u32 = 0x0a00_0103;
pub(super) const NETIMMERSE_VERSION_10_1_0_0: u32 = 0x0a01_0000;
pub(super) const NETIMMERSE_VERSION_10_2_0_0: u32 = 0x0a02_0000;
