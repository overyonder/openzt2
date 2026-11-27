use std::io;

pub(super) const MORPH_OPACITY_BINDING: u32 = 49;

const MORPH_OPACITY_SHADER: &str = r#"
fn openzt2_apply_morph_opacity(colour: vec4<f32>, position: vec4<f32>) -> vec4<f32> {
    if openzt2_morph_opacity.x <= 0.0 {
        discard;
    }
    if openzt2_morph_opacity.w != 0.0 {
        // Preserve non-linear blend modes on samples covered by this draw.
        let pattern = array<u32, 16>(0u, 8u, 2u, 10u, 12u, 4u, 14u, 6u, 3u, 11u, 1u, 9u, 15u, 7u, 13u, 5u);
        let cell = vec2<u32>(position.xy) % vec2<u32>(4u);
        if (f32(pattern[cell.y * 4u + cell.x]) + 0.5) / 16.0 > openzt2_morph_opacity.x {
            discard;
        }
        return colour;
    }
    let rgb = colour.rgb * mix(1.0, openzt2_morph_opacity.x, openzt2_morph_opacity.y);
    let alpha = mix(colour.a, 1.0, openzt2_morph_opacity.z) * openzt2_morph_opacity.x;
    return vec4(rgb, alpha);
}
"#;

fn append_morph_opacity_shader(source: &mut String) {
    use std::fmt::Write as _;

    let group = super::effect_pass_gpu_data::EFFECT_SHADER_BINDING_LAYOUT.descriptor_set;
    writeln!(
        source,
        "\n@group({group}) @binding({MORPH_OPACITY_BINDING}) var<uniform> openzt2_morph_opacity: vec4<f32>;"
    )
    .expect("writing to a String cannot fail");
    source.push_str(MORPH_OPACITY_SHADER);
}

const DEPTH_BIAS_SHADER: &str = r#"
struct D3d9ColourOutput {
    @location(0) colour: vec4<f32>,
    @if(D3D9_DEPTH_BIAS_BITS) @builtin(frag_depth) depth: f32,
}

@if(D3D9_DEPTH_BIAS_BITS)
fn d3d9_biased_fragment_depth(position: vec4<f32>) -> f32 {
    let constant_bias = bitcast<f32>(u32(constants::D3D9_DEPTH_BIAS_BITS));
    let slope_bias = bitcast<f32>(u32(constants::D3D9_SLOPE_BIAS_BITS));
    let maximum_slope = max(abs(dpdx(position.z)), abs(dpdy(position.z)));
    return clamp(position.z - constant_bias - slope_bias * maximum_slope, 0.0, 1.0);
}
"#;

pub(super) fn fixed_function_source_with_morph_opacity(source: &str) -> String {
    let mut source = source.replace(
        "output.colour = evaluate_fixed_function_texture_stages(fragment_inputs, false);",
        "output.colour = openzt2_apply_morph_opacity(evaluate_fixed_function_texture_stages(fragment_inputs, false), input.position);",
    );
    append_morph_opacity_shader(&mut source);
    source.push_str(DEPTH_BIAS_SHADER);
    source
}

/// Rewrites the single output return generated for a translated fragment entry.
pub(super) fn apply_morph_opacity_to_translated_fragment(source: &mut String) -> io::Result<()> {
    let Some(stage) = source.find("@fragment") else {
        return Err(io::Error::other(
            "translated fragment has no fragment entry",
        ));
    };
    let Some((signature, body)) = source[stage..].split_once('{') else {
        return Err(io::Error::other("translated fragment has no entry body"));
    };
    let Some((body, _)) = body.split_once("\n}") else {
        return Err(io::Error::other("translated fragment has no entry end"));
    };
    let Some((inputs, output)) = signature.rsplit_once("->") else {
        return Ok(());
    };
    let Some((before_return, value)) = body.rsplit_once("return ") else {
        return Err(io::Error::other("translated fragment has no output return"));
    };
    let Some((value, _)) = value.split_once(';') else {
        return Err(io::Error::other("translated fragment has no output value"));
    };
    let index = stage + signature.len() + 1 + before_return.len();
    let end = inputs
        .rfind(')')
        .ok_or_else(|| io::Error::other("translated fragment has no argument list"))?;
    let arguments = &signature[signature
        .find('(')
        .ok_or_else(|| io::Error::other("translated fragment has no arguments"))?
        + 1..end];
    let position = fragment_position(source, arguments);
    let position_name = position.as_deref().unwrap_or("openzt2_morph_position");
    let (replacement, biased_output, extra_output_type) = if output.contains("@location(0)") {
        (
            format!("var draw_output: D3d9ColourOutput;\n    draw_output.colour = openzt2_apply_morph_opacity({value}, {position_name});\n    @if(D3D9_DEPTH_BIAS_BITS) draw_output.depth = draw_depth;\n    return draw_output;"),
            Some("D3d9ColourOutput"),
            String::new(),
        )
    } else {
        let declaration = format!("struct {} {{", output.trim());
        let Some((_, members)) = source.split_once(&declaration) else {
            return Ok(());
        };
        let members = members
            .split_once('}')
            .map_or(members, |(members, _)| members);
        let Some(member) = members.lines().find_map(|line| {
            let (_, location) = line.split_once("@location(0)")?;
            location.split_once(':').map(|(name, _)| name.trim())
        }) else {
            return Ok(());
        };
        let colour = format!("var openzt2_morph_output = {value};\n    openzt2_morph_output.{member} = openzt2_apply_morph_opacity(openzt2_morph_output.{member}, {position_name});\n");
        if members.contains("@builtin(frag_depth)") {
            (
                colour + "    return openzt2_morph_output;",
                None,
                String::new(),
            )
        } else {
            use std::fmt::Write;
            let mut draw = colour + "    var draw_output: D3d9MrtOutput;\n";
            for declaration in members.lines().filter_map(|line| line.split_once(':')) {
                if let Some(field) = declaration.0.split_whitespace().last() {
                    writeln!(
                        draw,
                        "    draw_output.{field} = openzt2_morph_output.{field};"
                    )
                    .map_err(io::Error::other)?;
                }
            }
            draw += "    @if(D3D9_DEPTH_BIAS_BITS) draw_output.depth = draw_depth;\n    return draw_output;";
            let output_type = format!("\nstruct D3d9MrtOutput {{{members}\n    @if(D3D9_DEPTH_BIAS_BITS) @builtin(frag_depth) depth: f32,\n}}\n");
            (draw, Some("D3d9MrtOutput"), output_type)
        }
    };
    let has_arguments = !arguments.trim().is_empty();
    let output_start = signature.len() - output.len();
    let body_start = stage + signature.len() + 1;
    source.replace_range(
        index..index + "return ".len() + value.len() + 1,
        &replacement,
    );
    if let Some(biased_output) = biased_output {
        source.insert_str(body_start, &format!("\n    @if(D3D9_DEPTH_BIAS_BITS) let draw_depth = d3d9_biased_fragment_depth({position_name});"));
        source.replace_range(
            stage + output_start..body_start - 1,
            &format!(" {biased_output} "),
        );
    }
    if position.is_none() {
        source.insert_str(
            stage + end,
            if has_arguments {
                ", @builtin(position) openzt2_morph_position: vec4<f32>"
            } else {
                "@builtin(position) openzt2_morph_position: vec4<f32>"
            },
        );
    }
    append_morph_opacity_shader(source);
    source.push_str(DEPTH_BIAS_SHADER);
    source.push_str(&extra_output_type);
    Ok(())
}

fn fragment_position(source: &str, arguments: &str) -> Option<String> {
    let builtin_name = |declaration: &str| {
        declaration
            .split_once("@builtin(position)")
            .and_then(|(_, field)| {
                field
                    .split_once(':')
                    .map(|(name, _)| name.trim().to_owned())
            })
    };
    if let Some(name) = builtin_name(arguments) {
        return Some(name);
    }
    for argument in arguments.split(',') {
        let Some((name, kind)) = argument.split_once(':') else {
            continue;
        };
        let declaration = format!("struct {} {{", kind.trim());
        if let Some(field) = source
            .split_once(&declaration)
            .and_then(|(_, members)| members.split_once('}'))
            .and_then(|(members, _)| builtin_name(members))
        {
            return Some(format!("{}.{field}", name.trim()));
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::apply_morph_opacity_to_translated_fragment;

    #[test]
    fn adds_position_for_direct_colour_output() -> std::io::Result<()> {
        let mut shader = String::from(
            "@fragment\nfn main(@location(1) t: vec4<f32>) -> @location(0) vec4<f32> {\n    return t;\n}\n",
        );
        apply_morph_opacity_to_translated_fragment(&mut shader)?;
        assert!(
            shader.contains("t: vec4<f32>, @builtin(position) openzt2_morph_position: vec4<f32>")
        );
        assert!(shader.contains(
            "draw_output.colour = openzt2_apply_morph_opacity(t, openzt2_morph_position);"
        ));
        assert!(
            shader.contains("let draw_depth = d3d9_biased_fragment_depth(openzt2_morph_position);")
        );
        Ok(())
    }

    #[test]
    fn reuses_struct_position_and_preserves_depth_output() -> std::io::Result<()> {
        let mut shader = String::from(concat!(
            "struct In {\n    @builtin(position) pixel: vec4<f32>,\n}\n",
            "struct Out {\n    @location(0) colour: vec4<f32>,\n    @builtin(frag_depth) depth: f32,\n}\n",
            "@fragment\nfn main(inputs: In) -> Out {\n    return Out(vec4(1.0), 0.8);\n}\n",
        ));
        apply_morph_opacity_to_translated_fragment(&mut shader)?;
        assert!(shader.contains("fn main(inputs: In) -> Out"));
        assert!(shader
            .contains("openzt2_apply_morph_opacity(openzt2_morph_output.colour, inputs.pixel)"));
        assert!(shader.contains("var openzt2_morph_output = Out(vec4(1.0), 0.8);"));
        Ok(())
    }
}
