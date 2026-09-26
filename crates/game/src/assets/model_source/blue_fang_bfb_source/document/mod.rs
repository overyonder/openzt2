//! BFB document parsing and mesh queries.

use super::super::native_source_byte_reading::read_u32_little_endian;
use super::{
    collision_shape_parsing::{
        parse_blue_fang_bfb_capsule, parse_blue_fang_bfb_oriented_box, parse_blue_fang_bfb_sphere,
    },
    geometry_and_skin_parsing::{
        parse_blue_fang_bfb_mesh, parse_blue_fang_bfb_mesh_block,
        validate_blue_fang_bfb_mesh_blocks,
    },
    hierarchy_parsing_and_validation::{
        parse_blue_fang_bfb_hierarchy, validate_blue_fang_bfb_hierarchy,
    },
    object_block_framing::parse_blue_fang_bfb_object_block,
    source_error::BlueFangBfbSourceError,
    source_types::{
        BlueFangBfbCapsule, BlueFangBfbMesh, BlueFangBfbMeshBlock, BlueFangBfbNode,
        BlueFangBfbOrientedBox, BlueFangBfbSphere,
    },
};

const BLUE_FANG_BFB_HEADER: &[u8] = b"BFB!*000";

#[derive(Debug, Clone, PartialEq)]
pub(in super::super) struct BlueFangBfbDocument {
    pub(in super::super) source_path: String,
    meshes: Vec<BlueFangBfbMesh>,
    mesh_blocks: Vec<BlueFangBfbMeshBlock>,
    collision_boxes: Vec<BlueFangBfbOrientedBox>,
    collision_spheres: Vec<BlueFangBfbSphere>,
    collision_capsules: Vec<BlueFangBfbCapsule>,
    hierarchy: Vec<BlueFangBfbNode>,
}

impl BlueFangBfbDocument {
    pub(in super::super) fn parse(
        source_path: String,
        source_bytes: &[u8],
    ) -> Result<Self, BlueFangBfbSourceError> {
        if !source_bytes.starts_with(BLUE_FANG_BFB_HEADER) {
            return Err(BlueFangBfbSourceError::unsupported_format(source_path));
        }
        let mut header_cursor = 80;
        let object_block_count = read_u32_little_endian(
            source_bytes,
            &mut header_cursor,
            &source_path,
            "BFB block count",
        )?;
        let mut object_block_cursor = 88;
        let mut meshes = Vec::new();
        let mut mesh_blocks = Vec::new();
        let mut collision_boxes = Vec::new();
        let mut collision_spheres = Vec::new();
        let mut collision_capsules = Vec::new();
        for object_block_index in 0..object_block_count {
            let object_block =
                parse_blue_fang_bfb_object_block(source_bytes, object_block_cursor, &source_path)
                    .map_err(|error| error.with_object_block_context(object_block_index))?;
            match (
                object_block.block_type,
                object_block.name.eq_ignore_ascii_case("meshData"),
            ) {
                (6 | 8, true) => meshes.push(
                    parse_blue_fang_bfb_mesh(source_bytes, &object_block, &source_path)
                        .map_err(|error| error.with_object_block_context(object_block_index))?,
                ),
                (5 | 8, false) => mesh_blocks.push(
                    parse_blue_fang_bfb_mesh_block(source_bytes, &object_block, &source_path)
                        .map_err(|error| error.with_object_block_context(object_block_index))?,
                ),
                (3, false) if object_block.name.eq_ignore_ascii_case("orientedbox") => {
                    collision_boxes.push(
                        parse_blue_fang_bfb_oriented_box(source_bytes, &object_block, &source_path)
                            .map_err(|error| error.with_object_block_context(object_block_index))?,
                    );
                }
                (1, false) if object_block.name.eq_ignore_ascii_case("sphere") => {
                    collision_spheres.push(
                        parse_blue_fang_bfb_sphere(source_bytes, &object_block, &source_path)
                            .map_err(|error| error.with_object_block_context(object_block_index))?,
                    );
                }
                (4, false) if object_block.name.eq_ignore_ascii_case("capsule") => {
                    collision_capsules.push(
                        parse_blue_fang_bfb_capsule(source_bytes, &object_block, &source_path)
                            .map_err(|error| error.with_object_block_context(object_block_index))?,
                    );
                }
                _ => {}
            }
            object_block_cursor = object_block.block_end;
        }
        validate_blue_fang_bfb_mesh_blocks(&mesh_blocks, &meshes, &source_path)?;
        let hierarchy =
            parse_blue_fang_bfb_hierarchy(source_bytes, object_block_cursor, &source_path)?;
        validate_blue_fang_bfb_hierarchy(&hierarchy, &mesh_blocks, &source_path)?;
        Ok(Self {
            source_path,
            meshes,
            mesh_blocks,
            collision_boxes,
            collision_spheres,
            collision_capsules,
            hierarchy,
        })
    }

    pub(in super::super) fn meshes(&self) -> impl ExactSizeIterator<Item = &BlueFangBfbMesh> {
        self.meshes.iter()
    }

    pub(in super::super) fn mesh_blocks(
        &self,
    ) -> impl ExactSizeIterator<Item = &BlueFangBfbMeshBlock> {
        self.mesh_blocks.iter()
    }

    pub(in super::super) fn hierarchy(&self) -> impl ExactSizeIterator<Item = &BlueFangBfbNode> {
        self.hierarchy.iter()
    }

    pub(in super::super) fn collision_boxes(
        &self,
    ) -> impl ExactSizeIterator<Item = &BlueFangBfbOrientedBox> {
        self.collision_boxes.iter()
    }

    pub(in super::super) fn collision_spheres(
        &self,
    ) -> impl ExactSizeIterator<Item = &BlueFangBfbSphere> {
        self.collision_spheres.iter()
    }

    pub(in super::super) fn collision_capsules(
        &self,
    ) -> impl ExactSizeIterator<Item = &BlueFangBfbCapsule> {
        self.collision_capsules.iter()
    }
}
