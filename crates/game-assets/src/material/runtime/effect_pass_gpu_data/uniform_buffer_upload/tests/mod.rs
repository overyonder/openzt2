use super::*;

#[test]
fn sparse_registers_upload_only_the_active_shader_block() {
    let mut floats = D3d9FloatShaderRegisters {
        values: [Vec4::ZERO; 256],
    };
    floats.values[3] = Vec4::splat(3.0);
    floats.values[9] = Vec4::splat(9.0);
    let packed = pack_programmable_shader_registers(
        &floats,
        &D3d9IntegerShaderRegisters {
            values: [IVec4::ZERO; 16],
        },
        &D3d9BooleanShaderRegisters {
            values: [UVec4::ZERO; 4],
        },
        Some(&[vec![3, 9], vec![], vec![]]),
    );
    assert_eq!(packed.active_register_count, 2);
    assert_eq!(packed.values[0], Vec4::splat(3.0));
    assert_eq!(packed.values[1], Vec4::splat(9.0));
    assert_eq!(
        shader_buffer_from_packed_registers(&packed)
            .data
            .as_ref()
            .map(Vec::len),
        Some(32)
    );
}

#[test]
fn persistent_register_updates_match_encase_and_retain_storage() {
    let floats = D3d9FloatShaderRegisters {
        values: [Vec4::new(1.0, -0.0, 3.0, 4.0); 256],
    };
    let integers = D3d9IntegerShaderRegisters {
        values: [IVec4::new(-1, 2, -3, 4); 16],
    };
    let booleans = D3d9BooleanShaderRegisters {
        values: [UVec4::new(1, 0, 1, 0); 4],
    };
    for order in [
        None,
        Some([vec![9, 3], vec![2], vec![5, 0]]),
        Some([vec![], vec![], vec![]]),
    ] {
        let packed =
            pack_programmable_shader_registers(&floats, &integers, &booleans, order.as_ref());
        let expected = shader_buffer_from_packed_registers(&packed);
        let mut buffers = Assets::<ShaderBuffer>::default();
        let handle = buffers.add(shader_buffer_from_packed_registers(&packed));
        let pointer = buffers
            .get(&handle)
            .unwrap()
            .data
            .as_ref()
            .unwrap()
            .as_ptr();
        for _ in 0..2 {
            write_packed_registers_to_shader_buffer(
                &handle,
                &floats,
                &integers,
                &booleans,
                order.as_ref(),
                &mut buffers,
            );
            let actual = buffers.get(&handle).unwrap().data.as_ref().unwrap();
            assert_eq!(actual, expected.data.as_ref().unwrap());
            assert_eq!(actual.as_ptr(), pointer);
        }
    }
}
