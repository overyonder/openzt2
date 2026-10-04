use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum D3d9EffectProcessingError {
    #[error(
        "effect include {requested_include_path:?} from {parent_effect_path:?} could not be resolved"
    )]
    EffectIncludeCouldNotBeResolved {
        parent_effect_path: PathBuf,
        requested_include_path: PathBuf,
    },
    #[error("vkd3d-shader Effects failed: {0}")]
    Vkd3dShaderEffectCompilationFailed(String),
    #[error("vkd3d-shader Effects returned malformed output")]
    NativeDependencyReturnedMalformedOutput,
    #[error("effect parameter {parameter_name:?} assignment has {assignment_byte_count} bytes, but its native storage has {storage_byte_count} bytes")]
    ParameterAssignmentExceedsStorage {
        parameter_name: String,
        assignment_byte_count: usize,
        storage_byte_count: usize,
    },
    #[error("effect parameter {parameter_name:?} has object or sampler storage that cannot accept a raw numeric assignment")]
    ParameterAssignmentUnsupportedStorage { parameter_name: String },
    #[error("D3D9 shader translation failed: {0}")]
    D3d9ShaderTranslationFailed(String),
    #[error("compiled Effects evaluation failed: {0}")]
    CompiledEffectEvaluationFailed(String),
}
