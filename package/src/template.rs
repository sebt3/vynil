use handlebars::Handlebars;
use anyhow::Result;


pub fn template(template:&str ,values:&serde_json::Map<String, serde_json::Value>) -> Result<String> {
    let reg = Handlebars::new();
    Ok(reg.render_template(template, values)?)
}
