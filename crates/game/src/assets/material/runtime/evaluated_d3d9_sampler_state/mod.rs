use std::io;

use bevy::render::render_resource::{AddressMode, FilterMode, MipmapFilterMode, SamplerDescriptor};

#[derive(Clone, Copy, Debug)]
pub(super) struct EvaluatedD3d9SamplerState {
    address: [u32; 3],
    border_color: u32,
    mag_filter: u32,
    min_filter: u32,
    mip_filter: u32,
    lod_bias: f32,
    max_mip_level: u32,
    max_anisotropy: u32,
    srgb_texture: u32,
    element_index: u32,
}

impl Default for EvaluatedD3d9SamplerState {
    fn default() -> Self {
        Self {
            address: [1; 3],
            border_color: 0,
            mag_filter: 1,
            min_filter: 1,
            mip_filter: 0,
            lod_bias: 0.0,
            max_mip_level: 0,
            max_anisotropy: 1,
            srgb_texture: 0,
            element_index: 0,
        }
    }
}

impl EvaluatedD3d9SamplerState {
    pub(super) fn apply(&mut self, state: u32, value: u32) -> io::Result<()> {
        match state {
            1..=3 => self.address[state as usize - 1] = value,
            4 => self.border_color = value,
            5 => self.mag_filter = value,
            6 => self.min_filter = value,
            7 => self.mip_filter = value,
            8 => self.lod_bias = f32::from_bits(value),
            9 => self.max_mip_level = value,
            10 => self.max_anisotropy = value,
            11 => self.srgb_texture = value,
            12 => self.element_index = value,
            _ => return Err(invalid("state", state)),
        }
        Ok(())
    }

    pub(super) fn validate(&self) -> io::Result<()> {
        self.address
            .into_iter()
            .try_for_each(|value| address_mode(value).map(|_| ()))?;
        filter_mode(self.mag_filter)?;
        filter_mode(self.min_filter)?;
        mipmap_filter(self.mip_filter)?;
        if self.border_color != 0 {
            return Err(invalid("border colour", self.border_color));
        }
        if self.lod_bias != 0.0 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("unsupported D3D9 sampler LOD bias {}", self.lod_bias),
            ));
        }
        if self.max_anisotropy == 0 || self.max_anisotropy > u32::from(u16::MAX) {
            return Err(invalid("maximum anisotropy", self.max_anisotropy));
        }
        if self.srgb_texture != 0 {
            return Err(invalid("sRGB texture sampling", self.srgb_texture));
        }
        if self.element_index != 0 {
            return Err(invalid("element index", self.element_index));
        }
        Ok(())
    }

    pub(super) const fn samples_texture_with_srgb_decode(&self) -> bool {
        self.srgb_texture != 0
    }

    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_precision_loss,
        clippy::expect_used,
        reason = "validated D3D9 sampler state is retained privately and converted only after validate()"
    )]
    pub(super) fn descriptor(&self) -> SamplerDescriptor<'static> {
        let anisotropic = matches!(self.mag_filter | self.min_filter, 3);
        SamplerDescriptor {
            label: Some("D3D9 effect sampler"),
            address_mode_u: address_mode(self.address[0]).expect("validated sampler address"),
            address_mode_v: address_mode(self.address[1]).expect("validated sampler address"),
            address_mode_w: address_mode(self.address[2]).expect("validated sampler address"),
            mag_filter: if anisotropic {
                FilterMode::Linear
            } else {
                filter_mode(self.mag_filter).expect("validated sampler filter")
            },
            min_filter: if anisotropic {
                FilterMode::Linear
            } else {
                filter_mode(self.min_filter).expect("validated sampler filter")
            },
            mipmap_filter: mipmap_filter(self.mip_filter).expect("validated sampler mip filter"),
            lod_min_clamp: self.max_mip_level as f32,
            lod_max_clamp: 32.0,
            compare: None,
            anisotropy_clamp: if anisotropic {
                self.max_anisotropy as u16
            } else {
                1
            },
            border_color: None,
        }
    }
}

fn address_mode(value: u32) -> io::Result<AddressMode> {
    Ok(match value {
        1 => AddressMode::Repeat,
        2 => AddressMode::MirrorRepeat,
        3 => AddressMode::ClampToEdge,
        _ => return Err(invalid("address mode", value)),
    })
}

fn filter_mode(value: u32) -> io::Result<FilterMode> {
    Ok(match value {
        0 | 1 => FilterMode::Nearest,
        2 | 3 => FilterMode::Linear,
        _ => return Err(invalid("filter", value)),
    })
}

fn mipmap_filter(value: u32) -> io::Result<MipmapFilterMode> {
    Ok(match value {
        0 | 1 => MipmapFilterMode::Nearest,
        2 | 3 => MipmapFilterMode::Linear,
        _ => return Err(invalid("mipmap filter", value)),
    })
}

fn invalid(kind: &str, value: u32) -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidData,
        format!("unsupported D3D9 sampler {kind} {value}"),
    )
}
