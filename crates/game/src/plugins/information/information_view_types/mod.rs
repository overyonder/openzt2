use bevy::prelude::*;
use openzt2_game_data::ui_document::action::information::InformationViewCategory;

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct EntityEditorDataRootSurface;

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct InformationViewClass(pub(super) InformationViewCategory);

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct InformationViewClassResolved;

/// Visibility policy for the ten authored world-view categories.
#[derive(Resource, Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct InformationViewFilters([bool; 10]);

impl Default for InformationViewFilters {
    fn default() -> Self {
        Self([true; 10])
    }
}

impl InformationViewFilters {
    pub(super) fn category_is_visible(&self, category: InformationViewCategory) -> bool {
        self.0[information_view_category_index(category)]
    }

    pub(super) fn set_category_visibility(
        &mut self,
        category: InformationViewCategory,
        is_visible: bool,
    ) {
        self.0[information_view_category_index(category)] = is_visible;
    }
}

const fn information_view_category_index(category: InformationViewCategory) -> usize {
    match category {
        InformationViewCategory::Animals => 0,
        InformationViewCategory::Guests => 1,
        InformationViewCategory::Buildings => 2,
        InformationViewCategory::Entrances => 3,
        InformationViewCategory::Fences => 4,
        InformationViewCategory::Curbs => 5,
        InformationViewCategory::ZooWalls => 6,
        InformationViewCategory::Foliage => 7,
        InformationViewCategory::Shows => 8,
        InformationViewCategory::Tanks => 9,
    }
}
