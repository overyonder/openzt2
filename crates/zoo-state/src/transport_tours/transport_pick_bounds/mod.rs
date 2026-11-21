use bevy::{
    camera::primitives::Aabb, ecs::system::SystemParam,
    picking::mesh_picking::ray_cast::ray_aabb_intersection_3d, prelude::*, shape::Aabb3d,
};

#[derive(SystemParam)]
pub struct TransportPickBounds<'w, 's> {
    children: Query<'w, 's, &'static Children>,
    meshes: Query<
        'w,
        's,
        (
            &'static Aabb,
            &'static GlobalTransform,
            &'static InheritedVisibility,
        ),
    >,
}

impl TransportPickBounds<'_, '_> {
    pub fn hit_distance(&self, entity: Entity, ray: Ray3d) -> Option<f32> {
        std::iter::once(entity)
            .chain(self.children.iter_descendants(entity))
            .filter_map(|part| {
                let (bounds, transform, visibility) = self.meshes.get(part).ok()?;
                visibility
                    .get()
                    .then(|| {
                        ray_aabb_intersection_3d(
                            ray,
                            &Aabb3d::new(bounds.center, bounds.half_extents),
                            &transform.affine(),
                        )
                    })
                    .flatten()
            })
            .min_by(f32::total_cmp)
    }
}
