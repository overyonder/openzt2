//! Dispatches NIF blocks to their format parsers.

use super::{
    animation_controller_source_parsing::{
        parse_alpha_controller, parse_bone_level_of_detail_controller, parse_controller_sequence,
        parse_geometry_morpher_controller, parse_keyframe_controller, parse_keyframe_data,
        parse_material_colour_controller, parse_morph_data, parse_text_key_extra_data,
        parse_uv_controller, parse_visibility_controller,
    },
    block_payload::NetImmerseNifBlockPayload,
    collision_and_skin_source_parsing::{
        parse_collision_data, parse_skin_data, parse_skin_instance, parse_skin_partition_collection,
    },
    geometry_data_source_parsing::{parse_triangle_shape_data, parse_triangle_strips_data},
    interpolated_key_data_source_parsing::{
        parse_colour_data, parse_float_data, parse_position_data, parse_uv_data,
        parse_visibility_data,
    },
    particle_source_parsing::{
        parse_gravity_modifier, parse_particle_colour_modifier, parse_particle_growth_and_fade,
        parse_particle_mesh_modifier, parse_particle_meshes_data, parse_particle_rotation,
        parse_particle_system_controller, parse_particles_data, parse_planar_particle_collider,
    },
    render_property_and_texture_source_parsing::{
        parse_alpha_property, parse_boolean_extra_data, parse_depth_buffer_property,
        parse_flag_property, parse_integer_extra_data, parse_material_property, parse_pixel_data,
        parse_source_texture, parse_specular_property, parse_stencil_property,
        parse_string_extra_data, parse_texture_effect, parse_texturing_property,
        parse_vertex_colour_property,
    },
    scene_object_source_parsing::{
        parse_ambient_light, parse_billboard_node, parse_directional_light, parse_grouping_node,
        parse_level_of_detail_node, parse_particle_meshes_geometry, parse_particles_geometry,
        parse_point_light, parse_triangle_shape_geometry, parse_triangle_strips_geometry,
    },
    source_error::NetImmerseNifSourceError,
};

type Result<T> = std::result::Result<T, NetImmerseNifSourceError>;

pub(super) fn parse_block_payload(
    source_bytes: &[u8],
    cursor: &mut usize,
    source_path: &str,
    encoded_version: u32,
    block_type_name: &str,
) -> Result<NetImmerseNifBlockPayload> {
    match block_type_name {
        "NiControllerSequence" => {
            parse_controller_sequence(source_bytes, cursor, source_path, encoded_version)
                .map(NetImmerseNifBlockPayload::NiControllerSequence)
        }
        "NiNode" => parse_grouping_node(source_bytes, cursor, source_path)
            .map(NetImmerseNifBlockPayload::NiNode),
        "NiBillboardNode" => {
            parse_billboard_node(source_bytes, cursor, source_path, encoded_version)
                .map(NetImmerseNifBlockPayload::NiBillboardNode)
        }
        "NiLODNode" => {
            parse_level_of_detail_node(source_bytes, cursor, source_path, encoded_version)
                .map(NetImmerseNifBlockPayload::NiLODNode)
        }
        "NiAmbientLight" => parse_ambient_light(source_bytes, cursor, source_path)
            .map(NetImmerseNifBlockPayload::NiAmbientLight),
        "NiDirectionalLight" => parse_directional_light(source_bytes, cursor, source_path)
            .map(NetImmerseNifBlockPayload::NiDirectionalLight),
        "NiPointLight" => parse_point_light(source_bytes, cursor, source_path)
            .map(NetImmerseNifBlockPayload::NiPointLight),
        "NiZBufferProperty" => parse_depth_buffer_property(source_bytes, cursor, source_path)
            .map(NetImmerseNifBlockPayload::NiZBufferProperty),
        "NiVertexColorProperty" => parse_vertex_colour_property(source_bytes, cursor, source_path)
            .map(NetImmerseNifBlockPayload::NiVertexColorProperty),
        "NiStringExtraData" => parse_string_extra_data(source_bytes, cursor, source_path)
            .map(NetImmerseNifBlockPayload::NiStringExtraData),
        "NiTriStrips" => parse_triangle_strips_geometry(source_bytes, cursor, source_path)
            .map(NetImmerseNifBlockPayload::NiTriStrips),
        "NiTriShape" => parse_triangle_shape_geometry(source_bytes, cursor, source_path)
            .map(NetImmerseNifBlockPayload::NiTriShape),
        "NiParticleMeshes" => parse_particle_meshes_geometry(source_bytes, cursor, source_path)
            .map(NetImmerseNifBlockPayload::NiParticleMeshes),
        "NiIntegerExtraData" => parse_integer_extra_data(source_bytes, cursor, source_path)
            .map(NetImmerseNifBlockPayload::NiIntegerExtraData),
        "NiBooleanExtraData" => parse_boolean_extra_data(source_bytes, cursor, source_path)
            .map(NetImmerseNifBlockPayload::NiBooleanExtraData),
        "NiParticles" => parse_particles_geometry(source_bytes, cursor, source_path)
            .map(NetImmerseNifBlockPayload::NiParticles),
        "NiParticleSystemController" => {
            parse_particle_system_controller(source_bytes, cursor, source_path)
                .map(NetImmerseNifBlockPayload::NiParticleSystemController)
        }
        "NiParticleRotation" => parse_particle_rotation(source_bytes, cursor, source_path)
            .map(NetImmerseNifBlockPayload::NiParticleRotation),
        "NiParticleColorModifier" => {
            parse_particle_colour_modifier(source_bytes, cursor, source_path)
                .map(NetImmerseNifBlockPayload::NiParticleColorModifier)
        }
        "NiParticleGrowFade" => parse_particle_growth_and_fade(source_bytes, cursor, source_path)
            .map(NetImmerseNifBlockPayload::NiParticleGrowFade),
        "NiParticleMeshModifier" => parse_particle_mesh_modifier(source_bytes, cursor, source_path)
            .map(NetImmerseNifBlockPayload::NiParticleMeshModifier),
        "NiGravity" => parse_gravity_modifier(source_bytes, cursor, source_path)
            .map(NetImmerseNifBlockPayload::NiGravity),
        "NiPlanarCollider" => parse_planar_particle_collider(source_bytes, cursor, source_path)
            .map(NetImmerseNifBlockPayload::NiPlanarCollider),
        "NiColorData" => parse_colour_data(source_bytes, cursor, source_path)
            .map(NetImmerseNifBlockPayload::NiColorData),
        "NiTexturingProperty" => parse_texturing_property(source_bytes, cursor, source_path)
            .map(NetImmerseNifBlockPayload::NiTexturingProperty),
        "NiSourceTexture" => {
            parse_source_texture(source_bytes, cursor, source_path, encoded_version)
                .map(NetImmerseNifBlockPayload::NiSourceTexture)
        }
        "NiAlphaProperty" => parse_alpha_property(source_bytes, cursor, source_path)
            .map(NetImmerseNifBlockPayload::NiAlphaProperty),
        "NiMaterialProperty" => parse_material_property(source_bytes, cursor, source_path)
            .map(NetImmerseNifBlockPayload::NiMaterialProperty),
        "NiSpecularProperty" => parse_specular_property(source_bytes, cursor, source_path)
            .map(NetImmerseNifBlockPayload::NiSpecularProperty),
        "NiStencilProperty" => parse_stencil_property(source_bytes, cursor, source_path)
            .map(NetImmerseNifBlockPayload::NiStencilProperty),
        "NiDitherProperty" => {
            parse_flag_property(source_bytes, cursor, source_path, "NiDitherProperty")
                .map(NetImmerseNifBlockPayload::NiDitherProperty)
        }
        "NiShadeProperty" => {
            parse_flag_property(source_bytes, cursor, source_path, "NiShadeProperty")
                .map(NetImmerseNifBlockPayload::NiShadeProperty)
        }
        "NiWireframeProperty" => {
            parse_flag_property(source_bytes, cursor, source_path, "NiWireframeProperty")
                .map(NetImmerseNifBlockPayload::NiWireframeProperty)
        }
        "NiTriStripsData" => {
            parse_triangle_strips_data(source_bytes, cursor, source_path, encoded_version)
                .map(NetImmerseNifBlockPayload::NiTriStripsData)
        }
        "NiTriShapeData" => {
            parse_triangle_shape_data(source_bytes, cursor, source_path, encoded_version)
                .map(NetImmerseNifBlockPayload::NiTriShapeData)
        }
        "NiParticlesData" => parse_particles_data(source_bytes, cursor, source_path)
            .map(NetImmerseNifBlockPayload::NiParticlesData),
        "NiParticleMeshesData" => parse_particle_meshes_data(source_bytes, cursor, source_path)
            .map(NetImmerseNifBlockPayload::NiParticleMeshesData),
        "NiAlphaController" => parse_alpha_controller(source_bytes, cursor, source_path)
            .map(NetImmerseNifBlockPayload::NiAlphaController),
        "NiKeyframeController" => parse_keyframe_controller(source_bytes, cursor, source_path)
            .map(NetImmerseNifBlockPayload::NiKeyframeController),
        "NiMaterialColorController" => {
            parse_material_colour_controller(source_bytes, cursor, source_path, encoded_version)
                .map(NetImmerseNifBlockPayload::NiMaterialColorController)
        }
        "NiUVController" => parse_uv_controller(source_bytes, cursor, source_path)
            .map(NetImmerseNifBlockPayload::NiUVController),
        "NiGeomMorpherController" => {
            parse_geometry_morpher_controller(source_bytes, cursor, source_path, encoded_version)
                .map(NetImmerseNifBlockPayload::NiGeomMorpherController)
        }
        "NiVisController" => parse_visibility_controller(source_bytes, cursor, source_path)
            .map(NetImmerseNifBlockPayload::NiVisController),
        "NiKeyframeData" => parse_keyframe_data(source_bytes, cursor, source_path, encoded_version)
            .map(NetImmerseNifBlockPayload::NiKeyframeData),
        "NiFloatData" => parse_float_data(source_bytes, cursor, source_path)
            .map(NetImmerseNifBlockPayload::NiFloatData),
        "NiPosData" => parse_position_data(source_bytes, cursor, source_path)
            .map(NetImmerseNifBlockPayload::NiPosData),
        "NiUVData" => parse_uv_data(source_bytes, cursor, source_path)
            .map(NetImmerseNifBlockPayload::NiUVData),
        "NiVisData" => parse_visibility_data(source_bytes, cursor, source_path)
            .map(NetImmerseNifBlockPayload::NiVisData),
        "NiMorphData" => parse_morph_data(source_bytes, cursor, source_path)
            .map(NetImmerseNifBlockPayload::NiMorphData),
        "NiCollisionData" => {
            parse_collision_data(source_bytes, cursor, source_path, encoded_version)
                .map(NetImmerseNifBlockPayload::NiCollisionData)
        }
        "NiSkinData" => parse_skin_data(source_bytes, cursor, source_path, encoded_version)
            .map(NetImmerseNifBlockPayload::NiSkinData),
        "NiSkinInstance" => parse_skin_instance(source_bytes, cursor, source_path, encoded_version)
            .map(NetImmerseNifBlockPayload::NiSkinInstance),
        "NiSkinPartition" => {
            parse_skin_partition_collection(source_bytes, cursor, source_path, encoded_version)
                .map(NetImmerseNifBlockPayload::NiSkinPartition)
        }
        "NiTextKeyExtraData" => parse_text_key_extra_data(source_bytes, cursor, source_path)
            .map(NetImmerseNifBlockPayload::NiTextKeyExtraData),
        "NiTextureEffect" => {
            parse_texture_effect(source_bytes, cursor, source_path, encoded_version)
                .map(NetImmerseNifBlockPayload::NiTextureEffect)
        }
        "NiPixelData" => parse_pixel_data(source_bytes, cursor, source_path, encoded_version)
            .map(NetImmerseNifBlockPayload::NiPixelData),
        "NiBoneLODController" => {
            parse_bone_level_of_detail_controller(source_bytes, cursor, source_path)
                .map(NetImmerseNifBlockPayload::NiBoneLODController)
        }
        unsupported_block_type_name => Err(NetImmerseNifSourceError::unsupported_block_type(
            source_path,
            unsupported_block_type_name,
        )),
    }
}
