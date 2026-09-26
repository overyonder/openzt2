//! Material-asset construction for already evaluated native fixed-function state.

use std::io;

use bevy::{asset::LoadContext, prelude::*};

use super::{
    authored_material_parameter_lowering::{
        MaterialEffectParameterValue, OwnedMaterialEffectParameter,
    },
    material_asset_types::{EvaluatedTechniquePassMaterialAssets, MaterialAsset},
    runtime::effect_pass_material_asset_construction::create_effect_pass_material_assets_from_evaluated_technique,
};

pub(in crate::assets) fn create_material_asset_from_evaluated_d3d9_effect(
    evaluated_d3d9_effect: d3d9_effects::effect_types::EvaluatedD3d9Effect,
    texture_asset_paths: Vec<(String, String)>,
    evaluated_pass_label_prefix: &str,
    load_context: &mut LoadContext<'_>,
    texture_coordinate_animations: &std::sync::Arc<
        [super::runtime::texture_coordinate_animation::TextureCoordinateAnimation],
    >,
) -> io::Result<MaterialAsset> {
    let parameters = texture_asset_paths
        .iter()
        .map(
            |(parameter_name, texture_asset_path)| OwnedMaterialEffectParameter {
                parameter_name: parameter_name.clone(),
                parameter_value: MaterialEffectParameterValue::TextureAssetPath(Some(
                    texture_asset_path.clone(),
                )),
            },
        )
        .collect::<Vec<_>>();
    let evaluated_technique_pass_material_assets = evaluated_d3d9_effect
        .evaluated_techniques
        .iter()
        .enumerate()
        .filter(|(_, technique)| technique.is_valid)
        .map(|(technique_index, technique)| {
            create_effect_pass_material_assets_from_evaluated_technique(
                technique,
                &evaluated_d3d9_effect.parameter_descriptions,
                &parameters,
                load_context,
                &format!("{evaluated_pass_label_prefix}-technique-{technique_index}"),
                texture_coordinate_animations,
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
            "evaluated native material has no valid technique",
        ));
    }
    let bound_texture_assets = texture_asset_paths
        .into_iter()
        .map(|(_, path)| load_context.load(path))
        .collect();
    Ok(MaterialAsset {
        evaluated_d3d9_effect,
        _source_dependencies: Box::default(),
        evaluated_technique_pass_material_assets,
        bound_texture_assets,
    })
}
