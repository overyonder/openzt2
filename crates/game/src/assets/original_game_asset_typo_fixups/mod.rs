use crate::assets::source_document::{
    ordered_source_document_types::{OrderedSourceDocumentChild, OrderedSourceDocumentNode},
    ui::model::SourceUiWidgetKind,
};

pub(crate) fn corrected_cursor_filename(filename: &str) -> &str {
    // Mode XML names `rotateobj.cur`; the installed rotation cursor is `rotobj.cur`.
    if filename.eq_ignore_ascii_case("rotateobj.cur") {
        "rotobj.cur"
    } else {
        filename
    }
}

pub(crate) fn corrected_audio_clip_path(path: &str) -> Option<&'static str> {
    [
        (
            "entities/units/animals/shared/sound/generic_fo3otstep_dirt4.wav",
            "entities/units/animals/shared/sound/generic_footstep_dirt4.wav",
        ),
        (
            "entities/units/animals/ternarctic/sound/ternarctic_adult_f_call.wav",
            "entities/units/animals/ternarctic/sound/ternartic_adult_f_call.wav",
        ),
        ("sounds/b_bf_loop.wav", "sounds/b_bf_lop.wav"),
        (
            "entities/units/animals/thylacine/sound/thylacinel_adult_play_roar.wav",
            "entities/units/animals/thylacine/sound/thylacine_adult_play_roar.wav",
        ),
        (
            "entities/units/animals/thylacine/sound/thylacinel_young_play_roar.wav",
            "entities/units/animals/thylacine/sound/thylacine_young_play_roar.wav",
        ),
    ]
    .into_iter()
    .find_map(|(misspelled, corrected)| path.eq_ignore_ascii_case(misspelled).then_some(corrected))
}

/// Five award layouts use `<chlidren>` around ordinary UI widgets.
pub(crate) fn is_misspelled_children_wrapper(node: &OrderedSourceDocumentNode) -> bool {
    if node.name != "chlidren" || !node.attributes.is_empty() {
        return false;
    }
    let mut widget_count = 0;
    let structural_match = node.children.items().iter().all(|child| match child {
        OrderedSourceDocumentChild::Element(child) => {
            widget_count += 1;
            SourceUiWidgetKind::is_known_tag(&child.name)
        }
        OrderedSourceDocumentChild::Text(text) => text.value().trim().is_empty(),
        OrderedSourceDocumentChild::Comment => true,
        OrderedSourceDocumentChild::ProcessingInstruction => false,
    });
    structural_match && widget_count != 0
}
