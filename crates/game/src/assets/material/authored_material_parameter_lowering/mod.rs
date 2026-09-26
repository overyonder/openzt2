use std::io;

use super::source::D3d9EffectParameterAssignment;

#[derive(Clone, Debug)]
pub(crate) enum MaterialEffectParameterValue {
    Boolean(bool),
    Integer(i32),
    FloatingPoint(f32),
    FloatVector(Box<[f32]>),
    FloatMatrix(Box<[f32; 16]>),
    TextureAssetPath(Option<String>),
}

#[derive(Clone, Debug)]
pub(super) struct OwnedMaterialEffectParameter {
    pub(super) parameter_name: String,
    pub(super) parameter_value: MaterialEffectParameterValue,
}

impl OwnedMaterialEffectParameter {
    pub(super) fn borrowed_d3d9_effect_parameter_assignment(
        &self,
    ) -> d3d9_effects::effect_types::D3d9EffectParameterAssignment<'_> {
        use d3d9_effects::effect_types::D3d9EffectParameterValue;

        let parameter_value = match &self.parameter_value {
            MaterialEffectParameterValue::Boolean(boolean_value) => {
                D3d9EffectParameterValue::Boolean {
                    boolean_value: *boolean_value,
                }
            }
            MaterialEffectParameterValue::Integer(integer_value) => {
                D3d9EffectParameterValue::Integer {
                    integer_value: *integer_value,
                }
            }
            MaterialEffectParameterValue::FloatingPoint(floating_point_value) => {
                D3d9EffectParameterValue::FloatingPoint {
                    floating_point_value: *floating_point_value,
                }
            }
            MaterialEffectParameterValue::FloatVector(vector_components) => {
                D3d9EffectParameterValue::FloatVector { vector_components }
            }
            MaterialEffectParameterValue::FloatMatrix(matrix_components) => {
                D3d9EffectParameterValue::FloatMatrix {
                    matrix_components: &matrix_components[..],
                }
            }
            MaterialEffectParameterValue::TextureAssetPath(texture_asset_path) => {
                D3d9EffectParameterValue::TextureParameterReference {
                    parameter_name: texture_asset_path.as_deref().unwrap_or(""),
                }
            }
        };
        d3d9_effects::effect_types::D3d9EffectParameterAssignment {
            parameter_name: &self.parameter_name,
            parameter_value,
        }
    }
}

pub(super) fn lower_authored_material_effect_parameter(
    authored_parameter: D3d9EffectParameterAssignment,
) -> io::Result<OwnedMaterialEffectParameter> {
    let invalid_parameter = || {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!(
                "invalid {} parameter {}",
                authored_parameter.parameter_type, authored_parameter.parameter_name
            ),
        )
    };
    let authored_value = authored_parameter.authored_value.trim();
    let parameter_value = match authored_parameter
        .parameter_type
        .to_ascii_lowercase()
        .as_str()
    {
        "bool" => MaterialEffectParameterValue::Boolean(
            parse_authored_boolean(authored_value).ok_or_else(invalid_parameter)?,
        ),
        "int" => MaterialEffectParameterValue::Integer(
            authored_value.parse().map_err(|_| invalid_parameter())?,
        ),
        "dword" => MaterialEffectParameterValue::Integer(i32::from_ne_bytes(
            authored_value
                .parse::<u32>()
                .map_err(|_| invalid_parameter())?
                .to_ne_bytes(),
        )),
        "float" => MaterialEffectParameterValue::FloatingPoint(
            parse_authored_floating_point(authored_value).ok_or_else(invalid_parameter)?,
        ),
        "vector2" => MaterialEffectParameterValue::FloatVector(
            parse_authored_float_vector::<2>(authored_value)
                .ok_or_else(invalid_parameter)?
                .into(),
        ),
        "vector3" => MaterialEffectParameterValue::FloatVector(
            parse_authored_float_vector::<3>(authored_value)
                .ok_or_else(invalid_parameter)?
                .into(),
        ),
        "vector4" => MaterialEffectParameterValue::FloatVector(
            parse_authored_float_vector::<4>(authored_value)
                .ok_or_else(invalid_parameter)?
                .into(),
        ),
        "vector" => MaterialEffectParameterValue::FloatVector(
            parse_authored_floating_point_components(authored_value)
                .filter(|components| !components.is_empty() && components.len() <= 4)
                .ok_or_else(invalid_parameter)?
                .into_boxed_slice(),
        ),
        "matrix" | "matrix4x4" => MaterialEffectParameterValue::FloatMatrix(Box::new(
            parse_authored_float_vector::<16>(authored_value).ok_or_else(invalid_parameter)?,
        )),
        "texture" | "texture2d" | "texture3d" | "texturecube" => {
            MaterialEffectParameterValue::TextureAssetPath(
                (!authored_value.is_empty()).then(|| authored_value.to_owned()),
            )
        }
        "mode" => {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "material parameter {} uses unsupported authored mode value {}",
                    authored_parameter.parameter_name, authored_value
                ),
            ));
        }
        _ => return Err(invalid_parameter()),
    };
    Ok(OwnedMaterialEffectParameter {
        parameter_name: authored_parameter.parameter_name,
        parameter_value,
    })
}

fn parse_authored_boolean(authored_value: &str) -> Option<bool> {
    match authored_value.to_ascii_lowercase().as_str() {
        "true" | "1" => Some(true),
        "false" | "0" => Some(false),
        _ => None,
    }
}

fn parse_authored_floating_point(authored_value: &str) -> Option<f32> {
    authored_value.trim_end_matches(['f', 'F']).parse().ok()
}

fn parse_authored_float_vector<const COMPONENT_COUNT: usize>(
    authored_value: &str,
) -> Option<[f32; COMPONENT_COUNT]> {
    parse_authored_floating_point_components(authored_value)?
        .try_into()
        .ok()
}

fn parse_authored_floating_point_components(authored_value: &str) -> Option<Vec<f32>> {
    authored_value
        .trim_matches(|character| matches!(character, '(' | ')' | '{' | '}'))
        .split(|character: char| character == ',' || character.is_ascii_whitespace())
        .filter(|component| !component.is_empty())
        .map(parse_authored_floating_point)
        .collect()
}
