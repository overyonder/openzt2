//! Focused parsing of NIF render properties, textures, effects, and pixel data.

use super::{
    super::native_source_byte_reading::{
        read_f32_little_endian, read_i16_little_endian, read_i32_little_endian, read_sized_string,
        read_source_boolean, read_three_by_three_matrix, read_three_component_vector,
        read_u16_little_endian, read_u32_little_endian, read_u8,
    },
    collision_and_skin_source_reading::read_plane,
    counted_source_collection_reading::read_raw_source_bytes,
    render_property_and_texture_source_types::{
        NetImmerseMipMap, NetImmerseNiAlphaProperty, NetImmerseNiBooleanExtraData,
        NetImmerseNiFlagProperty, NetImmerseNiIntegerExtraData, NetImmerseNiMaterialProperty,
        NetImmerseNiPixelData, NetImmerseNiSourceTexture, NetImmerseNiSpecularProperty,
        NetImmerseNiStencilProperty, NetImmerseNiStringExtraData, NetImmerseNiTextureEffect,
        NetImmerseNiTexturingProperty, NetImmerseNiVertexColorProperty,
        NetImmerseNiZBufferProperty, NetImmerseShaderTextureSlot,
    },
    render_texture_source_reading::{
        read_optional_texture_slot, read_pixel_format, read_texture_slot,
    },
    scene_object_source_parsing::{
        parse_object_identity_and_controller_links, parse_transformable_scene_object,
    },
    source_error::NetImmerseNifSourceError,
    NETIMMERSE_VERSION_10_0_1_3, NETIMMERSE_VERSION_10_2_0_0,
};

type Result<T> = std::result::Result<T, NetImmerseNifSourceError>;

pub(super) fn parse_depth_buffer_property(
    source_bytes: &[u8],
    cursor: &mut usize,
    source_path: &str,
) -> Result<NetImmerseNiZBufferProperty> {
    Ok(NetImmerseNiZBufferProperty {
        object: parse_object_identity_and_controller_links(source_bytes, cursor, source_path)?,
        flags: read_u16_little_endian(
            source_bytes,
            cursor,
            source_path,
            "NiZBufferProperty flags",
        )?,
        function: read_u32_little_endian(
            source_bytes,
            cursor,
            source_path,
            "NiZBufferProperty function",
        )?,
    })
}

pub(super) fn parse_vertex_colour_property(
    source_bytes: &[u8],
    cursor: &mut usize,
    source_path: &str,
) -> Result<NetImmerseNiVertexColorProperty> {
    Ok(NetImmerseNiVertexColorProperty {
        object: parse_object_identity_and_controller_links(source_bytes, cursor, source_path)?,
        flags: read_u16_little_endian(
            source_bytes,
            cursor,
            source_path,
            "NiVertexColorProperty flags",
        )?,
        vertex_mode: read_u32_little_endian(
            source_bytes,
            cursor,
            source_path,
            "NiVertexColorProperty vertex mode",
        )?,
        lighting_mode: read_u32_little_endian(
            source_bytes,
            cursor,
            source_path,
            "NiVertexColorProperty lighting mode",
        )?,
    })
}

pub(super) fn parse_flag_property(
    source_bytes: &[u8],
    cursor: &mut usize,
    source_path: &str,
    property_name: &str,
) -> Result<NetImmerseNiFlagProperty> {
    Ok(NetImmerseNiFlagProperty {
        object: parse_object_identity_and_controller_links(source_bytes, cursor, source_path)?,
        flags: read_u16_little_endian(
            source_bytes,
            cursor,
            source_path,
            &format!("{property_name} flags"),
        )?,
    })
}

pub(super) fn parse_string_extra_data(
    source_bytes: &[u8],
    cursor: &mut usize,
    source_path: &str,
) -> Result<NetImmerseNiStringExtraData> {
    Ok(NetImmerseNiStringExtraData {
        name: read_sized_string(source_bytes, cursor, source_path, "NiStringExtraData name")?,
        string_data: read_sized_string(
            source_bytes,
            cursor,
            source_path,
            "NiStringExtraData string data",
        )?,
    })
}

pub(super) fn parse_integer_extra_data(
    source_bytes: &[u8],
    cursor: &mut usize,
    source_path: &str,
) -> Result<NetImmerseNiIntegerExtraData> {
    Ok(NetImmerseNiIntegerExtraData {
        name: read_sized_string(source_bytes, cursor, source_path, "NiIntegerExtraData name")?,
        value: read_u32_little_endian(
            source_bytes,
            cursor,
            source_path,
            "NiIntegerExtraData value",
        )?,
    })
}

pub(super) fn parse_boolean_extra_data(
    source_bytes: &[u8],
    cursor: &mut usize,
    source_path: &str,
) -> Result<NetImmerseNiBooleanExtraData> {
    Ok(NetImmerseNiBooleanExtraData {
        name: read_sized_string(source_bytes, cursor, source_path, "NiBooleanExtraData name")?,
        value: read_source_boolean(
            source_bytes,
            cursor,
            source_path,
            "NiBooleanExtraData value",
        )?,
    })
}

pub(super) fn parse_texturing_property(
    source_bytes: &[u8],
    cursor: &mut usize,
    source_path: &str,
) -> Result<NetImmerseNiTexturingProperty> {
    let object = parse_object_identity_and_controller_links(source_bytes, cursor, source_path)?;
    let flags = read_u16_little_endian(
        source_bytes,
        cursor,
        source_path,
        "NiTexturingProperty flags",
    )?;
    let apply_mode = read_u32_little_endian(
        source_bytes,
        cursor,
        source_path,
        "NiTexturingProperty apply mode",
    )?;
    let texture_count = read_u32_little_endian(
        source_bytes,
        cursor,
        source_path,
        "NiTexturingProperty texture count",
    )?;
    let base_texture =
        read_optional_texture_slot(source_bytes, cursor, source_path, "base texture")?;
    let dark_texture =
        read_optional_texture_slot(source_bytes, cursor, source_path, "dark texture")?;
    let detail_texture =
        read_optional_texture_slot(source_bytes, cursor, source_path, "detail texture")?;
    let gloss_texture =
        read_optional_texture_slot(source_bytes, cursor, source_path, "gloss texture")?;
    let glow_texture =
        read_optional_texture_slot(source_bytes, cursor, source_path, "glow texture")?;
    let bump_map_texture =
        read_optional_texture_slot(source_bytes, cursor, source_path, "bump texture")?;
    let decal_0_texture =
        read_optional_texture_slot(source_bytes, cursor, source_path, "decal0 texture")?;
    let shader_texture_count = read_u32_little_endian(
        source_bytes,
        cursor,
        source_path,
        "NiTexturingProperty shader texture count",
    )?;
    let shader_textures = (0..shader_texture_count)
        .map(|_| {
            if read_source_boolean(source_bytes, cursor, source_path, "shader texture used")? {
                Ok(NetImmerseShaderTextureSlot {
                    texture: Some(read_texture_slot(
                        source_bytes,
                        cursor,
                        source_path,
                        "shader texture",
                    )?),
                    map_index: Some(read_u32_little_endian(
                        source_bytes,
                        cursor,
                        source_path,
                        "shader texture map index",
                    )?),
                })
            } else {
                Ok(NetImmerseShaderTextureSlot {
                    texture: None,
                    map_index: None,
                })
            }
        })
        .collect::<Result<Vec<_>>>()?;
    Ok(NetImmerseNiTexturingProperty {
        object,
        flags,
        apply_mode,
        texture_count,
        base_texture,
        dark_texture,
        detail_texture,
        gloss_texture,
        glow_texture,
        bump_map_texture,
        decal_0_texture,
        shader_textures,
    })
}

pub(super) fn parse_source_texture(
    source_bytes: &[u8],
    cursor: &mut usize,
    source_path: &str,
    encoded_version: u32,
) -> Result<NetImmerseNiSourceTexture> {
    let object = parse_object_identity_and_controller_links(source_bytes, cursor, source_path)?;
    let use_external = read_u8(
        source_bytes,
        cursor,
        source_path,
        "NiSourceTexture use external",
    )?;
    let use_internal = (use_external == 0 && encoded_version <= NETIMMERSE_VERSION_10_0_1_3)
        .then(|| {
            read_u8(
                source_bytes,
                cursor,
                source_path,
                "NiSourceTexture use internal",
            )
        })
        .transpose()?;
    let (file_name, pixel_data_ref) = if use_external == 1 {
        (
            Some(read_sized_string(
                source_bytes,
                cursor,
                source_path,
                "NiSourceTexture external file",
            )?),
            None,
        )
    } else if use_internal == Some(0) {
        (None, None)
    } else {
        (
            None,
            Some(read_i32_little_endian(
                source_bytes,
                cursor,
                source_path,
                "NiSourceTexture pixel data ref",
            )?),
        )
    };
    Ok(NetImmerseNiSourceTexture {
        object,
        use_external,
        use_internal,
        file_name,
        pixel_data_ref,
        pixel_layout: read_u32_little_endian(
            source_bytes,
            cursor,
            source_path,
            "NiSourceTexture pixel layout",
        )?,
        use_mipmaps: read_u32_little_endian(
            source_bytes,
            cursor,
            source_path,
            "NiSourceTexture mipmap format",
        )?,
        alpha_format: read_u32_little_endian(
            source_bytes,
            cursor,
            source_path,
            "NiSourceTexture alpha format",
        )?,
        is_static: read_u8(
            source_bytes,
            cursor,
            source_path,
            "NiSourceTexture static flag",
        )?,
    })
}

pub(super) fn parse_alpha_property(
    source_bytes: &[u8],
    cursor: &mut usize,
    source_path: &str,
) -> Result<NetImmerseNiAlphaProperty> {
    Ok(NetImmerseNiAlphaProperty {
        object: parse_object_identity_and_controller_links(source_bytes, cursor, source_path)?,
        flags: read_u16_little_endian(source_bytes, cursor, source_path, "NiAlphaProperty flags")?,
        threshold: read_u8(
            source_bytes,
            cursor,
            source_path,
            "NiAlphaProperty threshold",
        )?,
    })
}

pub(super) fn parse_material_property(
    source_bytes: &[u8],
    cursor: &mut usize,
    source_path: &str,
) -> Result<NetImmerseNiMaterialProperty> {
    Ok(NetImmerseNiMaterialProperty {
        object: parse_object_identity_and_controller_links(source_bytes, cursor, source_path)?,
        flags: read_u16_little_endian(
            source_bytes,
            cursor,
            source_path,
            "NiMaterialProperty flags",
        )?,
        ambient: read_three_component_vector(
            source_bytes,
            cursor,
            source_path,
            "NiMaterialProperty ambient",
        )?,
        diffuse: read_three_component_vector(
            source_bytes,
            cursor,
            source_path,
            "NiMaterialProperty diffuse",
        )?,
        specular: read_three_component_vector(
            source_bytes,
            cursor,
            source_path,
            "NiMaterialProperty specular",
        )?,
        emissive: read_three_component_vector(
            source_bytes,
            cursor,
            source_path,
            "NiMaterialProperty emissive",
        )?,
        glossiness: read_f32_little_endian(
            source_bytes,
            cursor,
            source_path,
            "NiMaterialProperty glossiness",
        )?,
        alpha: read_f32_little_endian(
            source_bytes,
            cursor,
            source_path,
            "NiMaterialProperty alpha",
        )?,
    })
}

pub(super) fn parse_specular_property(
    source_bytes: &[u8],
    cursor: &mut usize,
    source_path: &str,
) -> Result<NetImmerseNiSpecularProperty> {
    Ok(NetImmerseNiSpecularProperty {
        object: parse_object_identity_and_controller_links(source_bytes, cursor, source_path)?,
        flags: read_u16_little_endian(
            source_bytes,
            cursor,
            source_path,
            "NiSpecularProperty flags",
        )?,
    })
}

pub(super) fn parse_stencil_property(
    source_bytes: &[u8],
    cursor: &mut usize,
    source_path: &str,
) -> Result<NetImmerseNiStencilProperty> {
    Ok(NetImmerseNiStencilProperty {
        object: parse_object_identity_and_controller_links(source_bytes, cursor, source_path)?,
        flags: read_u16_little_endian(
            source_bytes,
            cursor,
            source_path,
            "NiStencilProperty flags",
        )?,
        stencil_enabled: read_u8(
            source_bytes,
            cursor,
            source_path,
            "NiStencilProperty enabled",
        )?,
        stencil_function: read_u32_little_endian(
            source_bytes,
            cursor,
            source_path,
            "NiStencilProperty function",
        )?,
        stencil_ref: read_u32_little_endian(
            source_bytes,
            cursor,
            source_path,
            "NiStencilProperty ref",
        )?,
        stencil_mask: read_u32_little_endian(
            source_bytes,
            cursor,
            source_path,
            "NiStencilProperty mask",
        )?,
        fail_action: read_u32_little_endian(
            source_bytes,
            cursor,
            source_path,
            "NiStencilProperty fail action",
        )?,
        z_fail_action: read_u32_little_endian(
            source_bytes,
            cursor,
            source_path,
            "NiStencilProperty z fail action",
        )?,
        pass_action: read_u32_little_endian(
            source_bytes,
            cursor,
            source_path,
            "NiStencilProperty pass action",
        )?,
        draw_mode: read_u32_little_endian(
            source_bytes,
            cursor,
            source_path,
            "NiStencilProperty draw mode",
        )?,
    })
}

pub(super) fn parse_texture_effect(
    source_bytes: &[u8],
    cursor: &mut usize,
    source_path: &str,
    encoded_version: u32,
) -> Result<NetImmerseNiTextureEffect> {
    Ok(NetImmerseNiTextureEffect {
        av_object: parse_transformable_scene_object(source_bytes, cursor, source_path)?,
        model_projection_matrix: read_three_by_three_matrix(
            source_bytes,
            cursor,
            source_path,
            "NiTextureEffect matrix",
        )?,
        model_projection_translation: read_three_component_vector(
            source_bytes,
            cursor,
            source_path,
            "NiTextureEffect translation",
        )?,
        texture_filtering: read_u32_little_endian(
            source_bytes,
            cursor,
            source_path,
            "NiTextureEffect filtering",
        )?,
        texture_clamping: read_u32_little_endian(
            source_bytes,
            cursor,
            source_path,
            "NiTextureEffect clamping",
        )?,
        texture_type: read_u32_little_endian(
            source_bytes,
            cursor,
            source_path,
            "NiTextureEffect texture type",
        )?,
        coordinate_generation_type: read_u32_little_endian(
            source_bytes,
            cursor,
            source_path,
            "NiTextureEffect coordinate generation",
        )?,
        source_texture_ref: read_i32_little_endian(
            source_bytes,
            cursor,
            source_path,
            "NiTextureEffect source texture",
        )?,
        enable_plane: read_u8(
            source_bytes,
            cursor,
            source_path,
            "NiTextureEffect enable plane",
        )?,
        plane: read_plane(source_bytes, cursor, source_path, "NiTextureEffect plane")?,
        ps2_l: (encoded_version < NETIMMERSE_VERSION_10_2_0_0)
            .then(|| {
                read_i16_little_endian(source_bytes, cursor, source_path, "NiTextureEffect PS2 L")
            })
            .transpose()?,
        ps2_k: (encoded_version < NETIMMERSE_VERSION_10_2_0_0)
            .then(|| {
                read_i16_little_endian(source_bytes, cursor, source_path, "NiTextureEffect PS2 K")
            })
            .transpose()?,
    })
}

pub(super) fn parse_pixel_data(
    source_bytes: &[u8],
    cursor: &mut usize,
    source_path: &str,
    encoded_version: u32,
) -> Result<NetImmerseNiPixelData> {
    let pixel_format = read_pixel_format(source_bytes, cursor, source_path, encoded_version)?;
    let palette_ref =
        read_i32_little_endian(source_bytes, cursor, source_path, "NiPixelData palette ref")?;
    let mipmap_count = read_u32_little_endian(
        source_bytes,
        cursor,
        source_path,
        "NiPixelData mipmap count",
    )?;
    let bytes_per_pixel = read_u32_little_endian(
        source_bytes,
        cursor,
        source_path,
        "NiPixelData bytes per pixel",
    )?;
    let mipmaps = (0..mipmap_count)
        .map(|_| {
            Ok(NetImmerseMipMap {
                width: read_u32_little_endian(
                    source_bytes,
                    cursor,
                    source_path,
                    "NiPixelData mipmap width",
                )?,
                height: read_u32_little_endian(
                    source_bytes,
                    cursor,
                    source_path,
                    "NiPixelData mipmap height",
                )?,
                offset: read_u32_little_endian(
                    source_bytes,
                    cursor,
                    source_path,
                    "NiPixelData mipmap offset",
                )?,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    let pixel_byte_count = read_u32_little_endian(
        source_bytes,
        cursor,
        source_path,
        "NiPixelData pixel byte count",
    )?;
    let pixel_data = read_raw_source_bytes(
        source_bytes,
        cursor,
        source_path,
        pixel_byte_count,
        "NiPixelData pixels",
    )?;
    Ok(NetImmerseNiPixelData {
        pixel_format,
        palette_ref,
        mipmaps,
        bytes_per_pixel,
        pixel_data,
    })
}
