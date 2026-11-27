use std::io;

use bevy::{
    asset::{io::Reader, AssetLoader, LoadContext},
    prelude::*,
};

use super::{
    authored_material_parameter_lowering::{
        lower_authored_material_effect_parameter, MaterialEffectParameterValue,
        OwnedMaterialEffectParameter,
    },
    d3d9_effect_program_evaluation::{
        read_and_repair_winning_d3d9_effect_source, resolve_blue_fang_d3d9_effect_source_path,
        LiveD3d9EffectIncludeSourceCollector,
    },
    d3d9_effect_source_asset_loading::D3d9EffectSourceAsset,
    material_asset_types::{
        EvaluatedTechniquePassMaterialAssets, MaterialAsset, MaterialModeAsset, MaterialModeTarget,
        MaterialTextureAssetBinding,
    },
    runtime::effect_pass_material_asset_construction::create_effect_pass_material_assets_from_evaluated_technique,
    source::BlueFangMaterialSourceDocument,
};
use crate::asset_source::AssetArchives;

#[derive(TypePath)]
pub(super) struct MaterialAssetLoader {
    asset_archives: AssetArchives,
}

impl FromWorld for MaterialAssetLoader {
    fn from_world(world: &mut World) -> Self {
        Self {
            asset_archives: world.resource::<AssetArchives>().clone(),
        }
    }
}

impl AssetLoader for MaterialAssetLoader {
    type Asset = MaterialAsset;
    type Settings = ();
    type Error = io::Error;

    #[cfg_attr(
        feature = "tracing",
        tracing::instrument(name = "load_authored_material", skip_all)
    )]
    async fn load(
        &self,
        reader: &mut dyn Reader,
        _: &Self::Settings,
        load_context: &mut LoadContext<'_>,
    ) -> io::Result<Self::Asset> {
        let mut material_source_bytes = Vec::new();
        reader.read_to_end(&mut material_source_bytes).await?;
        let authored_material_source: BlueFangMaterialSourceDocument =
            quick_xml::de::from_reader(material_source_bytes.as_slice())
                .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
        let mut material_effect_parameters = authored_material_source
            .parameter_assignments
            .into_iter()
            .map(lower_authored_material_effect_parameter)
            .collect::<io::Result<Box<[_]>>>()?;
        resolve_material_texture_parameter_asset_paths(
            &self.asset_archives,
            load_context.path().path(),
            &mut material_effect_parameters,
        );

        let effect_source_path = resolve_blue_fang_d3d9_effect_source_path(
            authored_material_source
                .effect_source
                .as_deref()
                .map(str::trim)
                .filter(|effect_source| !effect_source.is_empty())
                .unwrap_or("Base"),
        );
        let mut modes = authored_material_source
            .modes
            .into_iter()
            .filter(|mode| !mode.name.is_empty() && !mode.material.is_empty())
            .map(|mode| MaterialModeAsset {
                name: mode.name.to_ascii_lowercase(),
                target: load_material_mode_target(&mode.material, load_context),
            })
            .collect::<Vec<_>>();
        if effect_source_path.to_string_lossy().contains("_wind") {
            modes.push(MaterialModeAsset {
                name: "caustic".to_owned(),
                target: load_material_mode_target("caustic_wind", load_context),
            });
        }
        let repaired_effect_source_bytes =
            read_and_repair_winning_d3d9_effect_source(&self.asset_archives, &effect_source_path)?;
        let borrowed_parameter_assignments = material_effect_parameters
            .iter()
            .map(OwnedMaterialEffectParameter::borrowed_d3d9_effect_parameter_assignment)
            .collect::<Vec<_>>();
        let mut include_source_collector =
            LiveD3d9EffectIncludeSourceCollector::new(&self.asset_archives);
        use d3d9_effects::{
            effect_compilation::compile_d3d9_effect_source_to_fx2_bytecode,
            effect_evaluation::evaluate_compiled_d3d9_effect,
            effect_types::{D3d9EffectParameterAssignment, D3d9EffectParameterValue},
        };
        let compiled = compile_d3d9_effect_source_to_fx2_bytecode(
            &effect_source_path,
            &repaired_effect_source_bytes,
            &mut include_source_collector,
        )
        .map_err(io::Error::other)?;
        let evaluated_d3d9_effect =
            evaluate_compiled_d3d9_effect(&compiled, &borrowed_parameter_assignments)
                .map_err(io::Error::other)?;
        let containment_parameters = ["CameraContainedCCW", "CameraContainedCW"].map(|name| {
            evaluated_d3d9_effect
                .parameter_descriptions
                .iter()
                .any(|description| {
                    description.parameter_name.eq_ignore_ascii_case(name)
                        || description
                            .semantic_name
                            .as_deref()
                            .is_some_and(|semantic| semantic.eq_ignore_ascii_case(name))
                })
                .then_some(name)
        });
        let containment_variants = if containment_parameters.iter().any(Option::is_some) {
            (0..4)
                .map(|flags| {
                    let mut assignments = borrowed_parameter_assignments.clone();
                    for (bit, name) in containment_parameters.iter().enumerate() {
                        if let Some(name) = name {
                            assignments.push(D3d9EffectParameterAssignment {
                                parameter_name: name,
                                parameter_value: D3d9EffectParameterValue::Boolean {
                                    boolean_value: flags & (1 << bit) != 0,
                                },
                            });
                        }
                    }
                    evaluate_compiled_d3d9_effect(&compiled, &assignments).map_err(io::Error::other)
                })
                .collect::<io::Result<Vec<_>>>()?
        } else {
            Vec::new()
        };
        let evaluated_technique_pass_material_assets = evaluated_d3d9_effect
            .evaluated_techniques
            .iter()
            .enumerate()
            .filter(|(_, technique)| technique.is_valid)
            .map(|(technique_index, technique)| {
                let containment_passes = containment_variants.iter().map(|effect| {
                    super::runtime::evaluated_d3d9_pass_state::collect_evaluated_d3d9_pass_states(&effect.evaluated_techniques[technique_index])
                }).collect::<io::Result<Vec<_>>>()?;
                let containment_cull_faces = (!containment_passes.is_empty()).then(|| {
                    (0..technique.evaluated_passes.len()).map(|index| {
                        std::array::from_fn(|flags| containment_passes[flags][index].cull)
                    }).collect::<Vec<_>>()
                });
                create_effect_pass_material_assets_from_evaluated_technique(
                    technique,
                    &evaluated_d3d9_effect.parameter_descriptions,
                    &material_effect_parameters,
                    &mut crate::asset_source::generated_asset_publication::GeneratedAssetPublication::Labeled(load_context),
                    &format!("technique-{technique_index}-effect-pass"),
                    &std::sync::Arc::default(),
                    &[],
                    containment_cull_faces.as_deref(),
                )
                .map(
                    |pass_material_assets| EvaluatedTechniquePassMaterialAssets {
                        quality_annotation: technique.quality_annotation,
                        pass_material_assets,
                    },
                )
            })
            .collect::<io::Result<Box<[_]>>>()?;
        if evaluated_technique_pass_material_assets.is_empty() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "effect has no valid technique",
            ));
        }
        let bound_texture_assets = material_effect_parameters
            .iter()
            .filter_map(|parameter| match &parameter.parameter_value {
                MaterialEffectParameterValue::TextureAssetPath(Some(texture_asset_path)) => {
                    Some(MaterialTextureAssetBinding {
                        parameter_name: parameter.parameter_name.clone(),
                        texture: load_context.load(texture_asset_path.clone()),
                    })
                }
                _ => None,
            })
            .collect();

        let include_source_paths = include_source_collector.finish();
        let mut effect_source_dependency_paths = Vec::with_capacity(include_source_paths.len() + 1);
        effect_source_dependency_paths.push(effect_source_path.clone());
        effect_source_dependency_paths.extend(include_source_paths);
        effect_source_dependency_paths.sort_unstable();
        effect_source_dependency_paths.dedup();
        let source_dependencies = effect_source_dependency_paths
            .iter()
            .map(|path| {
                load_context
                    .load::<D3d9EffectSourceAsset>(path.clone())
                    .untyped()
            })
            .collect();
        Ok(MaterialAsset {
            netimmerse_material_animations: Default::default(),
            netimmerse_texture_binding: None,
            authored_effect: Some(openzt2_game_data::AssetId::from_virtual_path(
                &effect_source_path.to_string_lossy(),
            )),
            evaluated_d3d9_effect,
            _source_dependencies: source_dependencies,
            evaluated_technique_pass_material_assets,
            bound_texture_assets,
            modes: modes.into_boxed_slice(),
        })
    }

    fn extensions(&self) -> &[&str] {
        &["bfmat"]
    }
}

fn load_material_mode_target(reference: &str, context: &mut LoadContext<'_>) -> MaterialModeTarget {
    let path = std::path::Path::new("Materials")
        .join(reference)
        .with_extension("bfmat");
    if path
        .to_string_lossy()
        .eq_ignore_ascii_case(&context.path().path().to_string_lossy())
    {
        MaterialModeTarget::ThisMaterial
    } else {
        MaterialModeTarget::Material(context.load(path))
    }
}

fn resolve_material_texture_parameter_asset_paths(
    asset_archives: &AssetArchives,
    material_asset_path: &std::path::Path,
    material_effect_parameters: &mut [OwnedMaterialEffectParameter],
) {
    for material_effect_parameter in material_effect_parameters {
        let MaterialEffectParameterValue::TextureAssetPath(Some(texture_asset_path)) =
            &mut material_effect_parameter.parameter_value
        else {
            continue;
        };
        if let Some(resolved_texture_asset_path) =
            asset_archives.resolve_model_texture_reference(material_asset_path, texture_asset_path)
        {
            *texture_asset_path = crate::texture::source_image_asset_path_selection::select_bevy_image_asset_path_for_blue_fang_source_image(
                &resolved_texture_asset_path.to_string_lossy(),
            );
        }
    }
}
