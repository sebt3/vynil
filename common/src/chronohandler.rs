use chrono::{DateTime, Local};
use rhai::ImmutableString;

#[derive(Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug)]
pub struct DateTimeHandler {
    pub date: DateTime<Local>,
}
impl DateTimeHandler {
    #[must_use]
    pub fn now() -> Self {
        Self { date: Local::now() }
    }

    pub fn format(&self, fmt: &str) -> String {
        format!("{}", self.date.format(fmt))
    }

    pub fn rhai_format(&mut self, fmt: String) -> ImmutableString {
        self.format(&fmt).into()
    }
}
