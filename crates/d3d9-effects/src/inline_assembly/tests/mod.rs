use super::rewrite_inline_d3d9_shader_assembly_blocks_for_fx2_compilation;

#[test]
fn assembly_keywords_in_comments_and_strings_leave_effect_source_unchanged(
) -> Result<(), crate::error::D3d9EffectProcessingError> {
    let source = br#"
        // VertexShader = asm { not a shader }
        /* PixelShader = asm { also not a shader } */
        string label = "PixelShader = asm { escaped \" quote }";
        technique T { pass P { ZEnable = true; } }
    "#;
    let rewritten = rewrite_inline_d3d9_shader_assembly_blocks_for_fx2_compilation(source)?;
    assert_eq!(rewritten.rewritten_effect_source, source);
    Ok(())
}
