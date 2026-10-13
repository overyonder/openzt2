//! Source-faithful NetImmerse triangle geometry-data records.

#[derive(Debug, Clone, PartialEq)]
pub(in super::super) struct NetImmerseNiTriStripsData {
    pub(in super::super) geometry: NetImmerseNiGeometryData,
    pub(in super::super) num_triangles: u16,
    pub(in super::super) strip_lengths: Vec<u16>,
    pub(in super::super) points: Option<Vec<Vec<u16>>>,
}
#[derive(Debug, Clone, PartialEq)]
pub(in super::super) struct NetImmerseNiTriShapeData {
    pub(in super::super) geometry: NetImmerseNiGeometryData,
    pub(in super::super) num_triangles: u16,
    pub(in super::super) num_triangle_points: u32,
    pub(in super::super) triangles: Option<Vec<[u16; 3]>>,
    pub(in super::super) match_groups: Vec<NetImmerseMatchGroup>,
}
#[derive(Debug, Clone, PartialEq)]
pub(in super::super) struct NetImmerseMatchGroup {
    pub(in super::super) vertex_indices: Vec<u16>,
}
#[derive(Debug, Clone, PartialEq)]
pub(in super::super) struct NetImmerseNiGeometryData {
    pub(in super::super) num_vertices: u16,
    pub(in super::super) vertices: Option<Vec<[f32; 3]>>,
    pub(in super::super) num_uv_sets: u8,
    pub(in super::super) extra_vectors_flags: u8,
    pub(in super::super) normals: Option<Vec<[f32; 3]>>,
    pub(in super::super) center: [f32; 3],
    pub(in super::super) radius: f32,
    pub(in super::super) vertex_colors: Option<Vec<[f32; 4]>>,
    pub(in super::super) uv_sets: Vec<Vec<[f32; 2]>>,
    pub(in super::super) consistency_flags: u16,
}
