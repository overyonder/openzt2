use bevy::ecs::entity::EntityHashSet;
use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use openzt2_world_state::model_render::ModelExpanded;

#[derive(Message)]
pub(crate) struct ModelBranchChanged(pub(crate) Entity);

pub(crate) fn register(app: &mut App) {
    if app
        .world()
        .contains_resource::<Messages<ModelBranchChanged>>()
    {
        return;
    }
    app.add_message::<ModelBranchChanged>()
        .add_observer(
            |event: On<Insert<(ChildOf, ModelExpanded)>>,
             mut changed: MessageWriter<ModelBranchChanged>| {
                changed.write(ModelBranchChanged(event.entity));
            },
        )
        .add_observer(
            |event: On<Remove<(ChildOf, ModelExpanded)>>,
             mut changed: MessageWriter<ModelBranchChanged>| {
                changed.write(ModelBranchChanged(event.entity));
            },
        );
}

#[derive(SystemParam)]
pub(super) struct ModelProjectionChanges<'w, 's> {
    branches: MessageReader<'w, 's, ModelBranchChanged>,
    children: Query<'w, 's, &'static Children>,
    stack: Local<'s, Vec<Entity>>,
    affected: Local<'s, EntityHashSet>,
    retry: Local<'s, Vec<Entity>>,
}

impl ModelProjectionChanges<'_, '_> {
    pub(super) fn collect(
        &mut self,
        branches: impl Iterator<Item = Entity>,
        primitives: impl Iterator<Item = Entity>,
    ) {
        self.affected.clear();
        self.stack
            .extend(self.branches.read().map(|event| event.0).chain(branches));
        while let Some(entity) = self.stack.pop() {
            if self.affected.insert(entity) {
                if let Ok(children) = self.children.get(entity) {
                    self.stack.extend(children.iter());
                }
            }
        }
        self.affected.extend(self.retry.drain(..).chain(primitives));
    }

    pub(super) fn candidates(&self) -> impl Iterator<Item = Entity> + '_ {
        self.affected.iter().copied()
    }

    pub(super) fn retry_missing(&mut self, missing: &mut Vec<Entity>) {
        self.retry.append(missing);
    }
}
