//! Source-faithful NetImmerse render-property, texture-effect, and pixel records.

use super::{
    collision_source_types::NetImmerseNiPlane,
    scene_object_source_types::{NetImmerseNiAvObject, NetImmerseNiObjectNet},
};

#[derive(Debug, Clone, PartialEq)]
pub(in super::super) struct NetImmerseNiZBufferProperty {
    pub(in super::super) object: NetImmerseNiObjectNet,
    pub(in super::super) flags: u16,
    pub(in super::super) function: u32,
}
#[derive(Debug, Clone, PartialEq)]
pub(in super::super) struct NetImmerseNiVertexColorProperty {
    pub(in super::super) object: NetImmerseNiObjectNet,
    pub(in super::super) flags: u16,
    pub(in super::super) vertex_mode: u32,
    pub(in super::super) lighting_mode: u32,
}
#[derive(Debug, Clone, PartialEq)]
pub(in super::super) struct NetImmerseNiFlagProperty {
    pub(in super::super) object: NetImmerseNiObjectNet,
    pub(in super::super) flags: u16,
}
#[derive(Debug, Clone, PartialEq)]
pub(in super::super) struct NetImmerseNiStringExtraData {
    pub(in super::super) name: String,
    pub(in super::super) string_data: String,
}
#[derive(Debug, Clone, PartialEq)]
pub(in super::super) struct NetImmerseNiIntegerExtraData {
    pub(in super::super) name: String,
    pub(in super::super) value: u32,
}
#[derive(Debug, Clone, PartialEq)]
pub(in super::super) struct NetImmerseNiBooleanExtraData {
    pub(in super::super) name: String,
    pub(in super::super) value: bool,
}
#[derive(Debug, Clone, PartialEq)]
pub(in super::super) struct NetImmerseNiTexturingProperty {
    pub(in super::super) object: NetImmerseNiObjectNet,
    pub(in super::super) flags: u16,
    pub(in super::super) apply_mode: u32,
    pub(in super::super) texture_count: u32,
    pub(in super::super) base_texture: Option<NetImmerseTextureSlot>,
    pub(in super::super) dark_texture: Option<NetImmerseTextureSlot>,
    pub(in super::super) detail_texture: Option<NetImmerseTextureSlot>,
    pub(in super::super) gloss_texture: Option<NetImmerseTextureSlot>,
    pub(in super::super) glow_texture: Option<NetImmerseTextureSlot>,
    pub(in super::super) bump_map_texture: Option<NetImmerseTextureSlot>,
    pub(in super::super) decal_0_texture: Option<NetImmerseTextureSlot>,
    pub(in super::super) shader_textures: Vec<NetImmerseShaderTextureSlot>,
}
#[derive(Debug, Clone, PartialEq)]
pub(in super::super) struct NetImmerseTextureSlot {
    pub(in super::super) source_ref: i32,
    pub(in super::super) clamp_mode: u32,
    pub(in super::super) filter_mode: u32,
    pub(in super::super) uv_set: u32,
    pub(in super::super) ps2_l: i16,
    pub(in super::super) ps2_k: i16,
}
#[derive(Debug, Clone, PartialEq)]
pub(in super::super) struct NetImmerseShaderTextureSlot {
    pub(in super::super) texture: Option<NetImmerseTextureSlot>,
    pub(in super::super) map_index: Option<u32>,
}
#[derive(Debug, Clone, PartialEq)]
pub(in super::super) struct NetImmerseNiSourceTexture {
    pub(in super::super) object: NetImmerseNiObjectNet,
    pub(in super::super) use_external: u8,
    pub(in super::super) use_internal: Option<u8>,
    pub(in super::super) file_name: Option<String>,
    pub(in super::super) pixel_data_ref: Option<i32>,
    pub(in super::super) pixel_layout: u32,
    pub(in super::super) use_mipmaps: u32,
    pub(in super::super) alpha_format: u32,
    pub(in super::super) is_static: u8,
}
#[derive(Debug, Clone, PartialEq)]
pub(in super::super) struct NetImmerseNiAlphaProperty {
    pub(in super::super) object: NetImmerseNiObjectNet,
    pub(in super::super) flags: u16,
    pub(in super::super) threshold: u8,
}
#[derive(Debug, Clone, PartialEq)]
pub(in super::super) struct NetImmerseNiMaterialProperty {
    pub(in super::super) object: NetImmerseNiObjectNet,
    pub(in super::super) flags: u16,
    pub(in super::super) ambient: [f32; 3],
    pub(in super::super) diffuse: [f32; 3],
    pub(in super::super) specular: [f32; 3],
    pub(in super::super) emissive: [f32; 3],
    pub(in super::super) glossiness: f32,
    pub(in super::super) alpha: f32,
}
#[derive(Debug, Clone, PartialEq)]
pub(in super::super) struct NetImmerseNiSpecularProperty {
    pub(in super::super) object: NetImmerseNiObjectNet,
    pub(in super::super) flags: u16,
}
#[derive(Debug, Clone, PartialEq)]
pub(in super::super) struct NetImmerseNiStencilProperty {
    pub(in super::super) object: NetImmerseNiObjectNet,
    pub(in super::super) flags: u16,
    pub(in super::super) stencil_enabled: u8,
    pub(in super::super) stencil_function: u32,
    pub(in super::super) stencil_ref: u32,
    pub(in super::super) stencil_mask: u32,
    pub(in super::super) fail_action: u32,
    pub(in super::super) z_fail_action: u32,
    pub(in super::super) pass_action: u32,
    pub(in super::super) draw_mode: u32,
}
#[derive(Debug, Clone, PartialEq)]
pub(in super::super) struct NetImmerseNiTextureEffect {
    pub(in super::super) av_object: NetImmerseNiAvObject,
    pub(in super::super) model_projection_matrix: [f32; 9],
    pub(in super::super) model_projection_translation: [f32; 3],
    pub(in super::super) texture_filtering: u32,
    pub(in super::super) texture_clamping: u32,
    pub(in super::super) texture_type: u32,
    pub(in super::super) coordinate_generation_type: u32,
    pub(in super::super) source_texture_ref: i32,
    pub(in super::super) enable_plane: u8,
    pub(in super::super) plane: NetImmerseNiPlane,
    pub(in super::super) ps2_l: Option<i16>,
    pub(in super::super) ps2_k: Option<i16>,
}
#[derive(Debug, Clone, PartialEq)]
pub(in super::super) struct NetImmerseNiPixelData {
    pub(in super::super) pixel_format: NetImmerseNiPixelFormat,
    pub(in super::super) palette_ref: i32,
    pub(in super::super) mipmaps: Vec<NetImmerseMipMap>,
    pub(in super::super) bytes_per_pixel: u32,
    pub(in super::super) pixel_data: Vec<u8>,
}
#[derive(Debug, Clone, PartialEq)]
pub(in super::super) struct NetImmerseNiPixelFormat {
    pub(in super::super) pixel_format: u32,
    pub(in super::super) red_mask: u32,
    pub(in super::super) green_mask: u32,
    pub(in super::super) blue_mask: u32,
    pub(in super::super) alpha_mask: u32,
    pub(in super::super) bits_per_pixel: u32,
    pub(in super::super) old_fast_compare: [u8; 8],
    pub(in super::super) tiling: Option<u32>,
}
#[derive(Debug, Clone, PartialEq)]
pub(in super::super) struct NetImmerseMipMap {
    pub(in super::super) width: u32,
    pub(in super::super) height: u32,
    pub(in super::super) offset: u32,
}
