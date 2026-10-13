use super::*;

pub(super) fn project_contained_object_transforms(
    occupancy: Res<InteractionContainerOccupancy>,
    objects: Query<(Entity, &ContainedObject)>,
    mut transforms: ParamSet<(
        bevy::transform::helper::TransformHelper,
        Query<&mut Transform>,
    )>,
) {
    for (item, contained) in &objects {
        let joint = contained.joint.or_else(|| {
            occupancy
                .container_for_member(item)
                .map(|(holder, _)| holder)
        });
        let Some(joint) =
            joint.and_then(|joint| transforms.p0().compute_global_transform(joint).ok())
        else {
            continue;
        };
        if let Ok(mut local) = transforms.p1().get_mut(item) {
            *local = joint
                .mul_transform(contained.relative_transform)
                .compute_transform();
        }
    }
}
