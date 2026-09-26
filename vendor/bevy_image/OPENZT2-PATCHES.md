# Bevy image patch

Source: crates.io `bevy_image` 0.19.1, with its Apache-2.0 and MIT licences.

`src/dds.rs` selects BGRA rather than RGBA after expanding legacy
`D3DFMT_R8G8B8` pixels. The source's BGR bytes and complete mip chain remain
unchanged apart from adding opaque alpha. The regression covers linear and
sRGB formats with two mip levels.
