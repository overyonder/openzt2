use std::fmt::Write;

use super::error::D3d9EffectProcessingError;

#[allow(
    clippy::redundant_pub_crate,
    reason = "the parent compiler module consumes this private child-module result"
)]
pub(super) struct D3d9EffectSourceWithCompiledInlineShaderAssembly {
    pub(super) rewritten_effect_source: Vec<u8>,
    compiled_inline_shaders_by_placeholder_marker: Vec<(u32, Box<[u8]>)>,
}

impl D3d9EffectSourceWithCompiledInlineShaderAssembly {
    pub(super) fn replace_placeholder_shaders_with_compiled_inline_shader_bytecode(
        &self,
        mut compiled_effect_bytecode: Vec<u8>,
    ) -> Result<Vec<u8>, D3d9EffectProcessingError> {
        for (placeholder_marker, compiled_inline_shader) in
            &self.compiled_inline_shaders_by_placeholder_marker
        {
            let placeholder_marker_bytes = placeholder_marker.to_le_bytes();
            let placeholder_marker_position = compiled_effect_bytecode
                .windows(placeholder_marker_bytes.len())
                .position(|window| window == placeholder_marker_bytes)
                .ok_or_else(|| {
                    D3d9EffectProcessingError::D3d9ShaderTranslationFailed(
                        "inline assembly placeholder marker was not emitted".to_owned(),
                    )
                })?;
            let placeholder_shader_start = compiled_effect_bytecode[..placeholder_marker_position]
                .windows(4)
                .enumerate()
                .filter(|(position, word)| {
                    if !matches!(
                        u32::from_le_bytes([word[0], word[1], word[2], word[3]]),
                        0xffff_0200 | 0xfffe_0200
                    ) || *position < 4
                    {
                        return false;
                    }
                    let size = u32::from_le_bytes([
                        compiled_effect_bytecode[position - 4],
                        compiled_effect_bytecode[position - 3],
                        compiled_effect_bytecode[position - 2],
                        compiled_effect_bytecode[position - 1],
                    ]) as usize;
                    position.checked_add(size).is_some_and(|end| {
                        end > placeholder_marker_position
                            && end <= compiled_effect_bytecode.len()
                            && end >= 4
                            && compiled_effect_bytecode[end - 4..end]
                                == 0x0000_ffff_u32.to_le_bytes()
                    })
                })
                .map(|(position, _)| position)
                .next()
                .ok_or_else(|| {
                    D3d9EffectProcessingError::D3d9ShaderTranslationFailed(
                        "inline assembly placeholder shader was not found".to_owned(),
                    )
                })?;
            let placeholder_shader_size_position = placeholder_shader_start
                .checked_sub(4)
                .ok_or(D3d9EffectProcessingError::NativeDependencyReturnedMalformedOutput)?;
            let recorded = u32::from_le_bytes(
                compiled_effect_bytecode
                    [placeholder_shader_size_position..placeholder_shader_start]
                    .try_into()
                    .map_err(|_| {
                        D3d9EffectProcessingError::NativeDependencyReturnedMalformedOutput
                    })?,
            ) as usize;
            let placeholder_shader_end = placeholder_shader_start
                .checked_add(recorded)
                .filter(|end| *end <= compiled_effect_bytecode.len())
                .ok_or_else(|| {
                    D3d9EffectProcessingError::D3d9ShaderTranslationFailed(
                        "inline assembly placeholder resource exceeded the effect".to_owned(),
                    )
                })?;
            let compiled_inline_shader_size = u32::try_from(compiled_inline_shader.len())
                .map_err(|_| D3d9EffectProcessingError::NativeDependencyReturnedMalformedOutput)?;
            compiled_effect_bytecode.splice(
                placeholder_shader_size_position..placeholder_shader_end,
                compiled_inline_shader_size
                    .to_le_bytes()
                    .into_iter()
                    .chain(compiled_inline_shader.iter().copied()),
            );
        }
        Ok(compiled_effect_bytecode)
    }
}

#[allow(
    clippy::redundant_pub_crate,
    reason = "the parent compiler module invokes this private child-module frontend"
)]
pub(super) fn rewrite_inline_d3d9_shader_assembly_blocks_for_fx2_compilation(
    effect_source: &[u8],
) -> Result<D3d9EffectSourceWithCompiledInlineShaderAssembly, D3d9EffectProcessingError> {
    let effect_source_text = std::str::from_utf8(effect_source)
        .map_err(|_| D3d9EffectProcessingError::NativeDependencyReturnedMalformedOutput)?;
    let inline_shader_assembly_blocks = find_inline_shader_assembly_blocks(effect_source_text)?;
    rewrite_compiled_inline_shader_assembly(effect_source_text, &inline_shader_assembly_blocks)
}

fn find_inline_shader_assembly_blocks(
    effect_source_text: &str,
) -> Result<Vec<(usize, usize, usize, bool)>, D3d9EffectProcessingError> {
    let source_code_bytes = mask_effect_source_comments_and_strings(effect_source_text.as_bytes())?;
    let effect_source_text = std::str::from_utf8(&source_code_bytes)
        .map_err(|_| D3d9EffectProcessingError::NativeDependencyReturnedMalformedOutput)?;
    let effect_source_bytes = effect_source_text.as_bytes();
    let mut inline_shader_assembly_blocks = Vec::new();
    let mut next_search_position = 0;
    while let Some(relative_assembly_keyword_position) =
        effect_source_text[next_search_position..].find("asm")
    {
        let assembly_keyword_position = next_search_position + relative_assembly_keyword_position;
        let byte_before_assembly_keyword = effect_source_bytes
            .get(assembly_keyword_position.wrapping_sub(1))
            .copied();
        let byte_after_assembly_keyword = effect_source_bytes
            .get(assembly_keyword_position + 3)
            .copied();
        if byte_before_assembly_keyword.is_some_and(is_d3d9_effect_identifier_byte)
            || byte_after_assembly_keyword.is_some_and(is_d3d9_effect_identifier_byte)
        {
            next_search_position = assembly_keyword_position + 3;
            continue;
        }
        let Some(assignment_operator_position) = effect_source_bytes[..assembly_keyword_position]
            .iter()
            .rposition(|byte| !byte.is_ascii_whitespace())
        else {
            next_search_position = assembly_keyword_position + 3;
            continue;
        };
        if effect_source_bytes[assignment_operator_position] != b'=' {
            next_search_position = assembly_keyword_position + 3;
            continue;
        }
        let opening_brace_position = effect_source_bytes[assembly_keyword_position + 3..]
            .iter()
            .position(|byte| !byte.is_ascii_whitespace())
            .map(|relative_position| assembly_keyword_position + 3 + relative_position)
            .filter(|position| effect_source_bytes[*position] == b'{')
            .ok_or(D3d9EffectProcessingError::NativeDependencyReturnedMalformedOutput)?;
        let closing_brace_position = find_matching_closing_brace_in_d3d9_effect_source(
            effect_source_bytes,
            opening_brace_position,
        )?;
        let shader_declaration = effect_source_text[..assignment_operator_position]
            .rsplit_once(';')
            .map_or(
                &effect_source_text[..assignment_operator_position],
                |(_, shader_declaration)| shader_declaration,
            );
        let is_vertex_shader = shader_declaration
            .split_ascii_whitespace()
            .any(|word| word.eq_ignore_ascii_case("VertexShader"));
        inline_shader_assembly_blocks.push((
            assembly_keyword_position,
            opening_brace_position,
            closing_brace_position,
            is_vertex_shader,
        ));
        next_search_position = closing_brace_position + 1;
    }
    Ok(inline_shader_assembly_blocks)
}

// Keep byte offsets stable while excluding keywords and braces in comments or strings.
fn mask_effect_source_comments_and_strings(
    source: &[u8],
) -> Result<Vec<u8>, D3d9EffectProcessingError> {
    let mut code = source.to_vec();
    let mut position = 0;
    let mut previous_code_byte = None;
    let mut assembly_opening_brace_pending = false;
    let mut assembly_brace_depth = 0_usize;
    while position < source.len() {
        let start = position;
        match source[position..] {
            [b';', ..] if assembly_brace_depth > 0 => {
                while position < source.len() && source[position] != b'\n' {
                    position += 1;
                }
            }
            [b'/', b'/', ..] => {
                position += 2;
                while position < source.len() && source[position] != b'\n' {
                    position += 1;
                }
            }
            [b'/', b'*', ..] => {
                let end = source[position + 2..]
                    .windows(2)
                    .position(|bytes| bytes == b"*/")
                    .ok_or(D3d9EffectProcessingError::NativeDependencyReturnedMalformedOutput)?;
                position += end + 4;
            }
            [quote @ (b'"' | b'\''), ..] => {
                position += 1;
                loop {
                    match source.get(position) {
                        Some(b'\\') => position += 2,
                        Some(byte) if *byte == quote => {
                            position += 1;
                            break;
                        }
                        Some(_) => position += 1,
                        None => {
                            return Err(
                                D3d9EffectProcessingError::NativeDependencyReturnedMalformedOutput,
                            )
                        }
                    }
                }
            }
            _ => {
                if is_d3d9_effect_identifier_byte(source[position]) {
                    while position < source.len()
                        && is_d3d9_effect_identifier_byte(source[position])
                    {
                        position += 1;
                    }
                    assembly_opening_brace_pending =
                        &source[start..position] == b"asm" && previous_code_byte == Some(b'=');
                    previous_code_byte = Some(source[position - 1]);
                    continue;
                }
                let byte = source[position];
                if !byte.is_ascii_whitespace() {
                    match byte {
                        b'{' if assembly_opening_brace_pending || assembly_brace_depth > 0 => {
                            assembly_brace_depth += 1;
                        }
                        b'}' if assembly_brace_depth > 0 => assembly_brace_depth -= 1,
                        _ => {}
                    }
                    assembly_opening_brace_pending = false;
                    previous_code_byte = Some(byte);
                }
                position += 1;
                continue;
            }
        }
        for byte in &mut code[start..position] {
            if !matches!(*byte, b'\r' | b'\n') {
                *byte = b' ';
            }
        }
    }
    Ok(code)
}

fn rewrite_compiled_inline_shader_assembly(
    effect_source_text: &str,
    inline_shader_assembly_blocks: &[(usize, usize, usize, bool)],
) -> Result<D3d9EffectSourceWithCompiledInlineShaderAssembly, D3d9EffectProcessingError> {
    let effect_source_bytes = effect_source_text.as_bytes();
    if inline_shader_assembly_blocks.is_empty() {
        return Ok(D3d9EffectSourceWithCompiledInlineShaderAssembly {
            rewritten_effect_source: effect_source_bytes.to_vec(),
            compiled_inline_shaders_by_placeholder_marker: Vec::new(),
        });
    }

    let mut rewritten_effect_source = String::new();
    let mut compiled_inline_shaders_by_placeholder_marker =
        Vec::with_capacity(inline_shader_assembly_blocks.len());
    for (
        inline_shader_index,
        (_, opening_brace_position, closing_brace_position, is_vertex_shader),
    ) in inline_shader_assembly_blocks.iter().copied().enumerate()
    {
        let placeholder_marker = 0x44fa_0000_u32
            + u32::try_from(inline_shader_index)
                .map_err(|_| D3d9EffectProcessingError::NativeDependencyReturnedMalformedOutput)?;
        let placeholder_float_value = f32::from_bits(placeholder_marker);
        if is_vertex_shader {
            writeln!(
                rewritten_effect_source,
                "float4 openzt2_inline_{inline_shader_index}() : POSITION {{ return float4({placeholder_float_value:?}, 0, 0, 1); }}"
            )
            .map_err(|_| D3d9EffectProcessingError::NativeDependencyReturnedMalformedOutput)?;
        } else {
            writeln!(
                rewritten_effect_source,
                "float4 openzt2_inline_{inline_shader_index}() : COLOR {{ return float4({placeholder_float_value:?}, 0, 0, 1); }}"
            )
            .map_err(|_| D3d9EffectProcessingError::NativeDependencyReturnedMalformedOutput)?;
        }
        let compiled_inline_shader =
            super::shader_translation::assemble_d3d9_shader_assembly_source_to_bytecode(
                &effect_source_bytes[opening_brace_position + 1..closing_brace_position],
            )?;
        compiled_inline_shaders_by_placeholder_marker
            .push((placeholder_marker, compiled_inline_shader));
    }
    let mut copied_source_end = 0;
    for (
        inline_shader_index,
        (assembly_keyword_position, _, closing_brace_position, is_vertex_shader),
    ) in inline_shader_assembly_blocks.iter().copied().enumerate()
    {
        rewritten_effect_source
            .push_str(&effect_source_text[copied_source_end..assembly_keyword_position]);
        let shader_profile = if is_vertex_shader { "vs_2_0" } else { "ps_2_0" };
        write!(
            rewritten_effect_source,
            "compile {shader_profile} openzt2_inline_{inline_shader_index}()"
        )
        .map_err(|_| D3d9EffectProcessingError::NativeDependencyReturnedMalformedOutput)?;
        copied_source_end = closing_brace_position + 1;
    }
    rewritten_effect_source.push_str(&effect_source_text[copied_source_end..]);
    Ok(D3d9EffectSourceWithCompiledInlineShaderAssembly {
        rewritten_effect_source: rewritten_effect_source.into_bytes(),
        compiled_inline_shaders_by_placeholder_marker,
    })
}

const fn is_d3d9_effect_identifier_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

fn find_matching_closing_brace_in_d3d9_effect_source(
    effect_source: &[u8],
    opening_brace_position: usize,
) -> Result<usize, D3d9EffectProcessingError> {
    let mut nested_brace_depth = 1_u32;
    for (position, byte) in effect_source
        .iter()
        .enumerate()
        .skip(opening_brace_position + 1)
    {
        match byte {
            b'{' => nested_brace_depth += 1,
            b'}' => {
                nested_brace_depth -= 1;
                if nested_brace_depth == 0 {
                    return Ok(position);
                }
            }
            _ => {}
        }
    }
    Err(D3d9EffectProcessingError::NativeDependencyReturnedMalformedOutput)
}

#[cfg(test)]
mod tests;
