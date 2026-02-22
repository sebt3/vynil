use chrono::{DateTime, Local};
use rhai::{ImmutableString, Engine};

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

pub fn chrono_rhai_register(engine: &mut Engine) {
    engine
        .register_type_with_name::<DateTimeHandler>("DateTimeHandler")
        .register_fn("date_now", DateTimeHandler::now)
        .register_fn("format", DateTimeHandler::rhai_format);
}
