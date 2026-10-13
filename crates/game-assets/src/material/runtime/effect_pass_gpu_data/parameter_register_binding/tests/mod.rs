use super::*;

#[test]
fn render_update_register_writes_preserve_bits_and_dirty_only_the_changed_stage() {
    let mut values = [Vec4::ZERO; 8];
    let mut dirty = 0;
    write_changed_float_registers(
        &mut values,
        2,
        &[Vec4::ONE],
        &mut dirty,
        D3d9ProgrammableShaderStage::Vertex,
    );
    assert_eq!(dirty, 1);
    dirty = 0;
    write_changed_float_registers(
        &mut values,
        2,
        &[Vec4::ONE],
        &mut dirty,
        D3d9ProgrammableShaderStage::Vertex,
    );
    assert_eq!(dirty, 0);
    write_changed_float_registers(
        &mut values,
        0,
        &[Vec4::splat(-0.0)],
        &mut dirty,
        D3d9ProgrammableShaderStage::Pixel,
    );
    assert_eq!(dirty, 2);
    assert_eq!(values[0].x.to_bits(), (-0.0_f32).to_bits());
    let before = values;
    dirty = 0;
    write_changed_float_registers(
        &mut values,
        8,
        &[Vec4::ONE],
        &mut dirty,
        D3d9ProgrammableShaderStage::Vertex,
    );
    assert_eq!(values, before);
    assert_eq!(dirty, 0);
}
