use std::borrow::Cow;

use gltf::json::{
    accessor::{Accessor, ComponentType, GenericComponentType, Type},
    buffer::{Buffer, Target, View},
    validation::{Checked, USize64},
    Index, Root,
};

use super::conversion_error::ConversionError;

#[derive(Default)]
pub(in crate::assets) struct GltfBinaryBuffer {
    bytes: Vec<u8>,
}

impl GltfBinaryBuffer {
    pub(in crate::assets) fn add_f32_accessor(
        &mut self,
        root: &mut Root,
        values: &[f32],
        accessor_type: Type,
        include_bounds: bool,
        target: Option<Target>,
    ) -> Index<Accessor> {
        let component_count = accessor_component_count(accessor_type);
        let bytes = values
            .iter()
            .flat_map(|value| value.to_le_bytes())
            .collect::<Vec<_>>();
        let buffer_view = self.add_buffer_view(root, &bytes, target);
        let (minimum, maximum) = (include_bounds && !values.is_empty())
            .then(|| component_bounds(values, component_count))
            .unzip();
        root.push(Accessor {
            buffer_view: Some(buffer_view),
            byte_offset: None,
            count: USize64((values.len() / component_count) as u64),
            component_type: Checked::Valid(GenericComponentType(ComponentType::F32)),
            extensions: None,
            extras: None,
            max: maximum.map(serde_json::Value::from),
            min: minimum.map(serde_json::Value::from),
            name: None,
            normalized: false,
            sparse: None,
            type_: Checked::Valid(accessor_type),
        })
    }

    pub(in crate::assets) fn add_u32_accessor(
        &mut self,
        root: &mut Root,
        values: &[u32],
        accessor_type: Type,
        target: Option<Target>,
    ) -> Index<Accessor> {
        let component_count = accessor_component_count(accessor_type);
        let bytes = values
            .iter()
            .flat_map(|value| value.to_le_bytes())
            .collect::<Vec<_>>();
        let buffer_view = self.add_buffer_view(root, &bytes, target);
        root.push(Accessor {
            buffer_view: Some(buffer_view),
            byte_offset: None,
            count: USize64((values.len() / component_count) as u64),
            component_type: Checked::Valid(GenericComponentType(ComponentType::U32)),
            extensions: None,
            extras: None,
            max: None,
            min: None,
            name: None,
            normalized: false,
            sparse: None,
            type_: Checked::Valid(accessor_type),
        })
    }

    pub(in crate::assets) fn add_u16_accessor(
        &mut self,
        root: &mut Root,
        values: &[u16],
        accessor_type: Type,
        target: Option<Target>,
    ) -> Index<Accessor> {
        let component_count = accessor_component_count(accessor_type);
        let bytes = values
            .iter()
            .flat_map(|value| value.to_le_bytes())
            .collect::<Vec<_>>();
        let buffer_view = self.add_buffer_view(root, &bytes, target);
        root.push(Accessor {
            buffer_view: Some(buffer_view),
            byte_offset: None,
            count: USize64((values.len() / component_count) as u64),
            component_type: Checked::Valid(GenericComponentType(ComponentType::U16)),
            extensions: None,
            extras: None,
            max: None,
            min: None,
            name: None,
            normalized: false,
            sparse: None,
            type_: Checked::Valid(accessor_type),
        })
    }

    fn add_buffer_view(
        &mut self,
        root: &mut Root,
        bytes: &[u8],
        target: Option<Target>,
    ) -> Index<View> {
        while self.bytes.len() % 4 != 0 {
            self.bytes.push(0);
        }
        let byte_offset = self.bytes.len();
        self.bytes.extend_from_slice(bytes);
        root.push(View {
            buffer: Index::new(0),
            byte_length: USize64(bytes.len() as u64),
            byte_offset: Some(USize64(byte_offset as u64)),
            byte_stride: None,
            name: None,
            target: target.map(Checked::Valid),
            extensions: None,
            extras: None,
        })
    }

    pub(in crate::assets) fn finish(self, mut root: Root) -> Result<Vec<u8>, ConversionError> {
        root.asset.generator = Some("OpenZT2".to_owned());
        root.buffers.push(Buffer {
            byte_length: USize64(self.bytes.len() as u64),
            name: None,
            uri: None,
            extensions: None,
            extras: None,
        });
        gltf::Document::from_json(root.clone())?;
        let json = serde_json::to_vec(&root)?;
        Ok(gltf::binary::Glb {
            header: gltf::binary::Header {
                magic: *b"glTF",
                version: 2,
                length: 0,
            },
            json: Cow::Owned(json),
            bin: Some(Cow::Owned(self.bytes)),
        }
        .to_vec()?)
    }
}

const fn accessor_component_count(accessor_type: Type) -> usize {
    match accessor_type {
        Type::Scalar => 1,
        Type::Vec2 => 2,
        Type::Vec3 => 3,
        Type::Vec4 | Type::Mat2 => 4,
        Type::Mat3 => 9,
        Type::Mat4 => 16,
    }
}

fn component_bounds(values: &[f32], component_count: usize) -> (Vec<f32>, Vec<f32>) {
    values.chunks_exact(component_count).fold(
        (
            vec![f32::INFINITY; component_count],
            vec![f32::NEG_INFINITY; component_count],
        ),
        |(mut minimum, mut maximum), value| {
            (0..component_count).for_each(|component| {
                minimum[component] = minimum[component].min(value[component]);
                maximum[component] = maximum[component].max(value[component]);
            });
            (minimum, maximum)
        },
    )
}
