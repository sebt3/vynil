use crate::{Error, Result};
use anyhow::Context;

use kube::runtime::events::{Event, EventType, Reporter};
use k8s_openapi::api::core::v1::ObjectReference;
use kube::{
    runtime::events::Recorder,
    Client,
};

pub fn from_error(error: &Error) -> Event {
    let mut str = format!("{error:}");
    str.truncate(100);
    Event {
        type_: EventType::Warning,
        reason: str.clone(),
        note: Some(format!("{error:}")),
        action: str,
        secondary: None,
    }
}
pub fn from(reason: String, action: String, note: Option<String>) -> Event {
    Event {
        type_: EventType::Normal,
        reason, note, action,
        secondary: None,
    }
}

pub fn get_reporter(manager: &str) -> Reporter {
    Reporter {
        controller: manager.into(),
        instance: Some(std::env::var("POD_NAME").unwrap_or_else(|_| "unknown".to_string())),
    }
}
pub fn get_empty_ref() -> ObjectReference {
    ObjectReference {
        api_version: None,
        field_path: None,
        kind: None,
        name: None,
        namespace: None,
        resource_version: None,
        uid: None
    }
}
pub async fn report(manager: &str, client: Client, e: Event, obj_ref: ObjectReference) -> Result<()> {
    let recorder = Recorder::new(client.clone(), get_reporter(manager), obj_ref);
    recorder.publish(e).await.with_context(|| "cannot report error".to_string())
}
