use std::{
    fmt,
    sync::{Arc, Mutex, MutexGuard},
};

use crate::{
    effect_types::D3d9EffectParameterAssignment,
    error::D3d9EffectProcessingError,
    native_abi::{
        openzt2_effect_evaluate_transform, openzt2_effect_open_parameter_scratch,
        openzt2_effect_parameter_count, openzt2_effect_transform_dependency_count,
        openzt2_effect_transform_dependency_index, openzt2_effect_transform_output_layout,
    },
};

use super::{
    mojoshader_effect_allocation::MojoShaderEffectAllocationOwner,
    parameter_application::apply_d3d9_effect_parameter_assignments_to_mojoshader_effect,
};

/// The native FX value layout, before the existing 16-component tail padding.
#[derive(Clone, Copy, Debug)]
pub struct D3d9TransformOutputLayout {
    pub parameter_class: u32,
    pub row_count: u32,
    pub column_count: u32,
    pub component_count: u32,
}

/// A fixed transform's compiled expression and its canonical FX parameter indices.
/// Keeps the native program alive after the initial evaluated commands are copied.
#[derive(Clone, Debug)]
pub struct RetainedD3d9TransformExpression {
    program: RetainedD3d9EffectProgram,
    technique_index: u32,
    pass_index: u32,
    state_index: u32,
    parameter_indices: Box<[u32]>,
    output_layout: D3d9TransformOutputLayout,
}

impl RetainedD3d9TransformExpression {
    /// Indices into the owning effect's `parameter_descriptions`, in native binding order.
    #[must_use]
    pub fn parameter_indices(&self) -> &[u32] {
        &self.parameter_indices
    }

    #[must_use]
    pub fn output_layout(&self) -> D3d9TransformOutputLayout {
        self.output_layout
    }

    /// Evaluates using the load-time defaults plus these semantic/name assignments.
    /// Each call owns fresh parameter and input/output register storage. The returned
    /// components keep the native FX layout and the initial command's tail padding.
    /// No source compilation, shader translation or shader binding occurs here.
    ///
    /// # Errors
    ///
    /// Returns parameter conversion, native allocation or expression evaluation errors.
    #[allow(
        unsafe_code,
        reason = "evaluates retained native instructions with owned scratch"
    )]
    pub fn evaluate(
        &self,
        parameter_assignments: &[D3d9EffectParameterAssignment<'_>],
    ) -> Result<[f32; 16], D3d9EffectProcessingError> {
        let allocation = self.program.lock_allocation()?;
        let scratch_pointer =
            unsafe { openzt2_effect_open_parameter_scratch(allocation.0.native_effect_pointer) };
        if scratch_pointer.is_null() {
            return Err(D3d9EffectProcessingError::CompiledEffectEvaluationFailed(
                "could not allocate fixed-transform parameter scratch".to_owned(),
            ));
        }
        // Drop scratch before the guard: its metadata borrows the retained allocation.
        let scratch = MojoShaderEffectAllocationOwner {
            native_effect_pointer: scratch_pointer,
        };
        apply_d3d9_effect_parameter_assignments_to_mojoshader_effect(
            scratch.native_effect_pointer,
            parameter_assignments,
        )?;
        let mut output = [0.0; 16];
        let result = unsafe {
            openzt2_effect_evaluate_transform(
                allocation.0.native_effect_pointer,
                scratch.native_effect_pointer,
                self.technique_index,
                self.pass_index,
                self.state_index,
                output.as_mut_ptr(),
            )
        };
        if result != 1 {
            let reason = match result {
                -1 => "could not allocate fixed-transform evaluation scratch",
                -2 => "fixed-transform expression has an unsupported output layout",
                -3 => "fixed-transform evaluation has no matching validated program owner",
                _ => "fixed-transform evaluator returned an unknown result",
            };
            return Err(D3d9EffectProcessingError::CompiledEffectEvaluationFailed(
                reason.to_owned(),
            ));
        }
        Ok(output)
    }
}

struct RetainedMojoShaderEffectAllocation(MojoShaderEffectAllocationOwner);

// SAFETY: shader.c uses owned malloc allocations and CPU-only MojoShader parse data,
// with the default allocator and no thread-affine graphics API handles. Its callbacks
// refer only to the same owned allocation. Moving it changes no native addresses.
// The wrapper never exposes its pointer outside this module's synchronous operations;
// all access, evaluation scratch destruction and final destruction are serialized by
// the Mutex/Arc lifetime. Native shader reference counters are never accessed concurrently.
// The compiled allocation is only initialized by the loading path; later evaluations
// read it and mutate exclusively their own parameter buffers and register storage.
#[allow(
    unsafe_code,
    reason = "audited CPU allocation transfer behind serialized ownership"
)]
unsafe impl Send for RetainedMojoShaderEffectAllocation {}

#[derive(Clone)]
pub(super) struct RetainedD3d9EffectProgram {
    allocation: Arc<Mutex<RetainedMojoShaderEffectAllocation>>,
}

impl fmt::Debug for RetainedD3d9EffectProgram {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RetainedD3d9EffectProgram")
            .finish_non_exhaustive()
    }
}

impl RetainedD3d9EffectProgram {
    pub(super) fn new(allocation: MojoShaderEffectAllocationOwner) -> Self {
        Self {
            allocation: Arc::new(Mutex::new(RetainedMojoShaderEffectAllocation(allocation))),
        }
    }

    fn lock_allocation(
        &self,
    ) -> Result<MutexGuard<'_, RetainedMojoShaderEffectAllocation>, D3d9EffectProcessingError> {
        self.allocation.lock().map_err(|_| {
            D3d9EffectProcessingError::CompiledEffectEvaluationFailed(
                "retained effect allocation lock was poisoned".to_owned(),
            )
        })
    }

    pub(super) fn copy_initial_evaluated_effect(
        &self,
    ) -> Result<crate::effect_types::EvaluatedD3d9Effect, D3d9EffectProcessingError> {
        let allocation = self.lock_allocation()?;
        super::evaluated_state_copying::copy_evaluated_d3d9_effect_from_mojoshader(
            &allocation.0,
            self,
        )
    }

    #[allow(
        unsafe_code,
        reason = "copies dependency indices and layout from the live FX owner"
    )]
    pub(super) fn retain_transform_expression(
        &self,
        allocation: &MojoShaderEffectAllocationOwner,
        technique_index: u32,
        pass_index: u32,
        state_index: u32,
    ) -> Result<Option<RetainedD3d9TransformExpression>, D3d9EffectProcessingError> {
        let pointer = allocation.native_effect_pointer;
        let count = unsafe {
            openzt2_effect_transform_dependency_count(
                pointer,
                technique_index,
                pass_index,
                state_index,
            )
        };
        if count == 0 {
            return Ok(None);
        }
        let parameter_count = unsafe { openzt2_effect_parameter_count(pointer) };
        let parameter_indices = (0..count)
            .map(|dependency| {
                let index = unsafe {
                    openzt2_effect_transform_dependency_index(
                        pointer,
                        technique_index,
                        pass_index,
                        state_index,
                        dependency,
                    )
                };
                (index < parameter_count)
                    .then_some(index)
                    .ok_or(D3d9EffectProcessingError::NativeDependencyReturnedMalformedOutput)
            })
            .collect::<Result<_, _>>()?;
        let mut layout = [0; 4];
        unsafe {
            openzt2_effect_transform_output_layout(
                pointer,
                technique_index,
                pass_index,
                state_index,
                layout.as_mut_ptr(),
            );
        }
        Ok(Some(RetainedD3d9TransformExpression {
            program: self.clone(),
            technique_index,
            pass_index,
            state_index,
            parameter_indices,
            output_layout: D3d9TransformOutputLayout {
                parameter_class: layout[0],
                row_count: layout[1],
                column_count: layout[2],
                component_count: layout[3],
            },
        }))
    }
}
