//! Custom Bevy mesh attributes emitted by native model and terrain lowering.

use bevy::{
    mesh::MeshVertexAttribute, platform::collections::HashMap,
    render::render_resource::VertexFormat,
};

pub(crate) const ATTRIBUTE_UV_2: MeshVertexAttribute =
    MeshVertexAttribute::new("Vertex_Uv_2", 8, VertexFormat::Float32x2);
pub(crate) const ATTRIBUTE_UV_EFFECTS: MeshVertexAttribute =
    MeshVertexAttribute::new("Vertex_UvEffects", 9, VertexFormat::Float32x3);
pub(crate) const ATTRIBUTE_AUXILIARY_VECTOR: MeshVertexAttribute =
    MeshVertexAttribute::new("Vertex_AuxiliaryVector", 10, VertexFormat::Float32x3);
pub(crate) const ATTRIBUTE_BIOME_INDEX: MeshVertexAttribute =
    MeshVertexAttribute::new("Vertex_BiomeIndex", 11, VertexFormat::Uint32);
pub(crate) const ATTRIBUTE_GROUND_COVER: MeshVertexAttribute =
    MeshVertexAttribute::new("Vertex_GroundCover", 12, VertexFormat::Uint32);
pub(crate) const ATTRIBUTE_WATER_TYPE: MeshVertexAttribute =
    MeshVertexAttribute::new("Vertex_WaterType", 13, VertexFormat::Uint32);

pub(super) fn custom_native_model_vertex_attributes() -> HashMap<Box<str>, MeshVertexAttribute> {
    HashMap::from([
        (Box::<str>::from("_UV_2"), ATTRIBUTE_UV_2),
        (Box::<str>::from("UV_2"), ATTRIBUTE_UV_2),
        (Box::<str>::from("_UV_EFFECTS"), ATTRIBUTE_UV_EFFECTS),
        (Box::<str>::from("UV_EFFECTS"), ATTRIBUTE_UV_EFFECTS),
        (
            Box::<str>::from("_AUXILIARY_VECTOR"),
            ATTRIBUTE_AUXILIARY_VECTOR,
        ),
        (
            Box::<str>::from("AUXILIARY_VECTOR"),
            ATTRIBUTE_AUXILIARY_VECTOR,
        ),
        (Box::<str>::from("_BIOME_INDEX"), ATTRIBUTE_BIOME_INDEX),
        (Box::<str>::from("BIOME_INDEX"), ATTRIBUTE_BIOME_INDEX),
        (Box::<str>::from("_GROUND_COVER"), ATTRIBUTE_GROUND_COVER),
        (Box::<str>::from("GROUND_COVER"), ATTRIBUTE_GROUND_COVER),
        (Box::<str>::from("_WATER_TYPE"), ATTRIBUTE_WATER_TYPE),
        (Box::<str>::from("WATER_TYPE"), ATTRIBUTE_WATER_TYPE),
    ])
}
