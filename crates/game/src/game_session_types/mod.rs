/// The rule set selected for a world session and retained with its saved world.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum WorldSessionMode {
    Freeform,
    Challenge,
    Campaign,
}
