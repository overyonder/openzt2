use std::io;

use openzt2_game_assets::source_document::ordered_source_document_types::OrderedSourceDocumentNode;
use openzt2_zoo_state::guests::guest_survey_types::GuestSurvey;

pub(super) fn read_original_saved_guest_survey(
    manager: &OrderedSourceDocumentNode,
) -> io::Result<GuestSurvey> {
    let mut survey = GuestSurvey {
        critical_need_countdown_seconds: scalar(manager, "CriticalNeedCountdown")?,
        previous_day_of_month: integer(manager, "PreviousDayOfMonth")?,
        current_day_of_month: integer(manager, "CurrentDayOfMonth")?,
        ..Default::default()
    };
    for (history, name) in survey.ratings.iter_mut().zip([
        "Sat",
        "Happiness",
        "Ent",
        "Edu",
        "Food",
        "Drink",
        "Dessert",
        "FavFood",
        "Donate",
        "Gift",
        "Bathroom",
    ]) {
        history.cumulative_rating = scalar(manager, &format!("kCum{name}Rating"))?;
        history.cumulative_count = unsigned(manager, &format!("kCumNum{name}Ratings"))?;
        let ratings = child(manager, &format!("k{name}Ratings"))?;
        let counts = child(manager, &format!("kNum{name}Ratings"))?;
        for day in 0..31 {
            let key = format!("M{}", day + 1);
            history.daily_ratings[day] = scalar(ratings, &key)?;
            history.daily_counts[day] = unsigned(counts, &key)?;
        }
    }
    Ok(survey)
}

fn child<'a>(
    node: &'a OrderedSourceDocumentNode,
    name: &str,
) -> io::Result<&'a OrderedSourceDocumentNode> {
    let mut children = node.element_children().filter(|child| child.name == name);
    let value = children
        .next()
        .ok_or_else(|| invalid(format!("{} has no {name}", node.name)))?;
    if children.next().is_some() {
        return Err(invalid(format!("{} repeats {name}", node.name)));
    }
    Ok(value)
}

fn required<'a>(node: &'a OrderedSourceDocumentNode, name: &str) -> io::Result<&'a str> {
    node.attribute(name)
        .ok_or_else(|| invalid(format!("{} has no {name}", node.name)))
}
fn scalar(node: &OrderedSourceDocumentNode, name: &str) -> io::Result<f32> {
    required(node, name)?
        .parse::<f32>()
        .ok()
        .filter(|value| value.is_finite())
        .ok_or_else(|| invalid(format!("{} {name} is not finite f32", node.name)))
}
fn integer(node: &OrderedSourceDocumentNode, name: &str) -> io::Result<i32> {
    required(node, name)?
        .parse()
        .map_err(|_| invalid(format!("{} {name} is not i32", node.name)))
}
fn unsigned(node: &OrderedSourceDocumentNode, name: &str) -> io::Result<u32> {
    required(node, name)?
        .parse::<u32>()
        .or_else(|_| integer(node, name).map(|value| u32::from_ne_bytes(value.to_ne_bytes())))
        .map_err(|_| invalid(format!("{} {name} is not a 32-bit count", node.name)))
}
fn invalid(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.into())
}
