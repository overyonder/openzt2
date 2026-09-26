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
    #[error("D3D9 shader translation failed: {0}")]
    D3d9ShaderTranslationFailed(String),
    #[error("compiled Effects evaluation failed: {0}")]
    CompiledEffectEvaluationFailed(String),
}
