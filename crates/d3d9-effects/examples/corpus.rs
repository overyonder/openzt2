use std::{
    env, fs, io,
    path::{Path, PathBuf},
};

use d3d9_effects::{
    effect_types::{D3d9EffectIncludeResolver, EvaluatedD3d9EffectCommand},
    error::D3d9EffectProcessingError,
};
use z2f::ArchiveSet;

struct Z2fOverlayD3d9EffectIncludeResolver<'a>(&'a ArchiveSet);

impl D3d9EffectIncludeResolver for Z2fOverlayD3d9EffectIncludeResolver<'_> {
    fn open(
        &mut self,
        parent: &Path,
        requested: &Path,
    ) -> Result<(PathBuf, Vec<u8>), D3d9EffectProcessingError> {
        let path = self
            .0
            .resolve_effect_include(parent, &requested.to_string_lossy())
            .ok_or_else(
                || D3d9EffectProcessingError::EffectIncludeCouldNotBeResolved {
                    parent_effect_path: parent.to_owned(),
                    requested_include_path: requested.to_owned(),
                },
            )?;
        let bytes = self.0.read(&path).map_err(|_| {
            D3d9EffectProcessingError::EffectIncludeCouldNotBeResolved {
                parent_effect_path: parent.to_owned(),
                requested_include_path: requested.to_owned(),
            }
        })?;
        let (_, source) =
            z2f::text::decoding::decode_blue_fang_source_text_from_utf8_or_utf16_bytes(&bytes)
                .map_err(
                    |_| D3d9EffectProcessingError::EffectIncludeCouldNotBeResolved {
                        parent_effect_path: parent.to_owned(),
                        requested_include_path: requested.to_owned(),
                    },
                )?;
        let source =
            z2f::text::d3d9_effect_syntax::repair_observed_blue_fang_d3d9_effect_source_syntax(
                &path, &source,
            )
            .into_bytes();
        Ok((path, source))
    }
}

fn append_z2f_archive_paths_from_file_or_directory(
    path: &Path,
    output_archive_paths: &mut Vec<PathBuf>,
) -> io::Result<()> {
    if path.is_file() {
        if path
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("z2f"))
        {
            output_archive_paths.push(path.to_owned());
        }
        return Ok(());
    }
    for entry in fs::read_dir(path)? {
        let path = entry?.path();
        if path.is_dir() {
            append_z2f_archive_paths_from_file_or_directory(&path, output_archive_paths)?;
        } else if path
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("z2f"))
        {
            output_archive_paths.push(path);
        }
    }
    Ok(())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut enabled_archive_paths = Vec::new();
    for argument in env::args_os().skip(1) {
        let mut argument_archive_paths = Vec::new();
        append_z2f_archive_paths_from_file_or_directory(
            Path::new(&argument),
            &mut argument_archive_paths,
        )?;
        argument_archive_paths.sort_unstable_by(|left_path, right_path| {
            left_path
                .to_string_lossy()
                .to_ascii_lowercase()
                .cmp(&right_path.to_string_lossy().to_ascii_lowercase())
        });
        enabled_archive_paths.extend(argument_archive_paths);
    }
    let archive_overlay = ArchiveSet::open(enabled_archive_paths)?;
    let resolved_d3d9_effect_entries = archive_overlay
        .resolved_winning_entries()
        .1
        .into_iter()
        .filter(|entry| {
            entry
                .normalized_asset_path
                .extension()
                .is_some_and(|extension| extension.eq_ignore_ascii_case("fx"))
        })
        .collect::<Vec<_>>();
    let mut failed_effect_or_shader_count = 0_usize;
    let mut translated_shader_count = 0_usize;
    for entry in &resolved_d3d9_effect_entries {
        let source = archive_overlay.read(&entry.normalized_asset_path)?;
        let (_, source) =
            z2f::text::decoding::decode_blue_fang_source_text_from_utf8_or_utf16_bytes(&source)?;
        let source =
            z2f::text::d3d9_effect_syntax::repair_observed_blue_fang_d3d9_effect_source_syntax(
                &entry.normalized_asset_path,
                &source,
            );
        let mut include_resolver = Z2fOverlayD3d9EffectIncludeResolver(&archive_overlay);
        match d3d9_effects::effect_evaluation::compile_and_evaluate_d3d9_effect_source(
            &entry.normalized_asset_path,
            source.as_bytes(),
            &mut include_resolver,
            &[],
        ) {
            Ok(effect) => {
                for shader in
                    effect
                        .evaluated_techniques
                        .iter()
                        .flat_map(|technique| &technique.evaluated_passes)
                        .flat_map(|pass| &pass.evaluated_commands)
                        .filter_map(|command| match command {
                            EvaluatedD3d9EffectCommand::D3d9VertexShaderBytecode {
                                shader_bytecode,
                            }
                            | EvaluatedD3d9EffectCommand::D3d9PixelShaderBytecode {
                                shader_bytecode,
                            } => Some(shader_bytecode),
                            _ => None,
                        })
                {
                    if let Err(error) =
                        d3d9_effects::shader_translation::translate_d3d9_shader_bytecode_to_spirv(
                            shader,
                            &SHADER_BINDING_LAYOUT,
                        )
                    {
                        failed_effect_or_shader_count += 1;
                        println!("{}: {error}", entry.normalized_asset_path.display());
                    } else {
                        translated_shader_count += 1;
                    }
                }
            }
            Err(error) => {
                failed_effect_or_shader_count += 1;
                println!("{}: {error}", entry.normalized_asset_path.display());
            }
        }
    }
    eprintln!(
        "evaluated {} effects and translated {} shaders; {} failed",
        resolved_d3d9_effect_entries.len(),
        translated_shader_count,
        failed_effect_or_shader_count,
    );
    if failed_effect_or_shader_count == 0 {
        Ok(())
    } else {
        Err("effect corpus compilation failed".into())
    }
}

const SHADER_BINDING_LAYOUT: d3d9_effects::shader_types::D3d9ShaderBindingLayout =
    d3d9_effects::shader_types::D3d9ShaderBindingLayout {
        descriptor_set: 3,
        vertex_uniform_binding: 18,
        pixel_uniform_binding: 27,
        first_texture_binding: 2,
        texture_binding_count: 8,
        first_sampler_binding: 10,
        first_cube_texture_binding: 38,
    };
