//! Shared construction of Bevy's glTF loader for lowered source assets.

use bevy::{
    gltf::{
        convert_coordinates::GltfConvertCoordinates, extensions::GltfExtensionHandlers,
        DefaultGltfImageSampler, GltfLoader, GltfSkinnedMeshBoundsPolicy,
    },
    prelude::*,
};

use super::custom_vertex_attributes::custom_native_model_vertex_attributes;

pub(in crate::assets) fn create_bevy_gltf_loader_for_lowered_source_assets(
    world: &mut World,
) -> GltfLoader {
    GltfLoader {
        supported_compressed_formats: world
            .get_resource::<bevy::image::CompressedImageFormatSupport>()
            .map_or(bevy::image::CompressedImageFormats::NONE, |formats| {
                formats.0
            }),
        custom_vertex_attributes: custom_native_model_vertex_attributes(),
        default_sampler: world.resource::<DefaultGltfImageSampler>().get_internal(),
        default_convert_coordinates: GltfConvertCoordinates::default(),
        extensions: world.resource::<GltfExtensionHandlers>().0.clone(),
        default_skinned_mesh_bounds_policy: GltfSkinnedMeshBoundsPolicy::default(),
    }
}
