use std::io;

pub(super) const MORPH_OPACITY_BINDING: u32 = 48;

const MORPH_OPACITY_SHADER: &str = r#"
@group(3) @binding(48) var<uniform> openzt2_morph_opacity: vec4<f32>;

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

pub(super) fn fixed_function_source_with_morph_opacity(source: &str) -> String {
    let mut source = source.replace(
        "return evaluate_fixed_function_texture_stages(fragment_inputs, false);",
        "return openzt2_apply_morph_opacity(evaluate_fixed_function_texture_stages(fragment_inputs, false), input.position);",
    );
    source.push_str(MORPH_OPACITY_SHADER);
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
    let replacement = if output.contains("@location(0)") {
        format!("return openzt2_apply_morph_opacity({value}, {position_name});")
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
        format!(
            "var openzt2_morph_output = {value};\n    openzt2_morph_output.{member} = openzt2_apply_morph_opacity(openzt2_morph_output.{member}, {position_name});\n    return openzt2_morph_output;"
        )
    };
    let has_arguments = !arguments.trim().is_empty();
    source.replace_range(
        index..index + "return ".len() + value.len() + 1,
        &replacement,
    );
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
    source.push_str(MORPH_OPACITY_SHADER);
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
        assert!(shader.contains("return openzt2_apply_morph_opacity(t, openzt2_morph_position);"));
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
