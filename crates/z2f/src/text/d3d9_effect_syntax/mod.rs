use std::path::Path;

/// Repairs the one malformed Effects statement observed in the shipped
/// expansion archives. The match includes the complete statement so valid
/// Effects syntax is otherwise preserved byte-for-byte.
#[must_use]
pub fn repair_observed_blue_fang_d3d9_effect_source_syntax(path: &Path, text: &str) -> String {
    if !path
        .extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("fx"))
    {
        return text.to_owned();
    }
    let repaired = text
        .replace(
            "ColorOp[1] = SelectArg1\r\n",
            "ColorOp[1] = SelectArg1;\r\n",
        )
        .replace("ColorOp[1] = SelectArg1\n", "ColorOp[1] = SelectArg1;\n");
    if path
        .file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| {
            name.eq_ignore_ascii_case("BaseGlow_fres.fx")
                || name.eq_ignore_ascii_case("BaseGlow_fres_vtx.fx")
        })
        && repaired.contains("Texture[1] = (lutTexture)")
        && !repaired.contains("texture lutTexture")
    {
        format!("{GLOW_LUT_DECLARATIONS}\n{repaired}")
    } else {
        repaired
    }
}

const GLOW_LUT_DECLARATIONS: &str = r"
sampler lutSampler = sampler_state
{
    MinFilter = LINEAR;
    MagFilter = LINEAR;
    MipFilter = POINT;
    AddressU = CLAMP;
    AddressV = CLAMP;
};
texture lutTexture : TextureLUT = NULL;
dword lutTexCoordIndex : TexCoordIndexLUT = 1;
float4x4 vecLocalToWorld : VecToWorld;
float4x4 vecWorldToView : VecWorldToView;
float4x4 lutScaleOffset : LUTScaleOffset;
";

#[cfg(test)]
mod tests {
    use super::repair_observed_blue_fang_d3d9_effect_source_syntax;
    use std::path::Path;

    #[test]
    fn repairs_observed_effect_semicolon() {
        assert_eq!(
            repair_observed_blue_fang_d3d9_effect_source_syntax(
                Path::new("effects/BaseGlossReflect_fres.fx"),
                "ColorOp[1] = SelectArg1\r\nAlphaArg1[1] = Texture;\r\n",
            ),
            "ColorOp[1] = SelectArg1;\r\nAlphaArg1[1] = Texture;\r\n"
        );
    }

    #[test]
    fn restores_observed_glow_lut_declarations() {
        let repaired = repair_observed_blue_fang_d3d9_effect_source_syntax(
            Path::new("effects/BaseGlow_fres.fx"),
            "technique T { pass P { Texture[1] = (lutTexture); } }",
        );
        assert!(repaired.contains("texture lutTexture : TextureLUT = NULL;"));
        assert!(repaired.contains("sampler lutSampler = sampler_state"));
    }
}
