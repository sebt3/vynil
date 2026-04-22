use rhai::{Engine, ImmutableString};
use wildmatch::WildMatch;

fn glob(text: ImmutableString, pattern: ImmutableString) -> bool {
    WildMatch::new(&pattern.to_string()).matches(&text.to_string())
}

pub fn glob_rhai_register(engine: &mut Engine) {
    engine
        .register_fn("glob", glob);
}
