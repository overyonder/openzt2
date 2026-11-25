use bevy::prelude::*;
use openzt2_interface_state::information::entity_selection_types::Inspectable;
use openzt2_ui::authored_ui_layout_participation::UiAuthoredLayoutDisplay;
use openzt2_zoo_state::animal_health::types::{Disease, Rampaging};

/// The original shows the find-animal buttons from `animalfindbuttons.xml`,
/// authored hidden, while any animal is diseased or rampaging.
pub(crate) fn project_animal_alerts_to_find_animal_buttons(
    diseased: Query<(), (With<Disease>, With<Inspectable>)>,
    rampaging: Query<(), (With<Rampaging>, With<Inspectable>)>,
    mut buttons: Query<(&Name, &UiAuthoredLayoutDisplay, &mut Node, &mut Visibility)>,
) {
    let any_diseased = !diseased.is_empty();
    let any_rampaging = !rampaging.is_empty();
    for (name, authored_display, mut node, mut visibility) in &mut buttons {
        let shown = match name.as_str() {
            "diseased_animal_button" => any_diseased,
            "rampaging_animal_button" => any_rampaging,
            _ => continue,
        };
        visibility.set_if_neq(if shown {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        });
        let display = if shown {
            authored_display.authored_display()
        } else {
            Display::None
        };
        if node.display != display {
            node.display = display;
        }
    }
}
