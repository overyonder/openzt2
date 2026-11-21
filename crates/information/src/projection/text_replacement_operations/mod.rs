use bevy::{
    prelude::{Mut, Text},
    text::EditableText,
};

/// Replaces projected UI text while retaining the existing allocation when
/// the projected value has not changed.
pub(crate) fn replace_projected_ui_text_if_changed(
    mut projected_ui_text: Mut<Text>,
    current_domain_text: &str,
) {
    if projected_ui_text.0 == current_domain_text {
        return;
    }
    projected_ui_text.0.clear();
    projected_ui_text.0.push_str(current_domain_text);
}

pub(crate) fn replace_projected_editable_ui_text_if_changed(
    mut projected_ui_text: Mut<EditableText>,
    current_domain_text: &str,
) {
    if projected_ui_text.value() == current_domain_text {
        return;
    }
    projected_ui_text.clear();
    projected_ui_text.editor_mut().set_text(current_domain_text);
}
