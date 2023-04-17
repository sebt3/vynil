use kube::runtime::events::{Event, EventType};
use k8s_openapi::api::core::v1::ObjectReference;

#[must_use] pub fn from_create(src_type: &str, src_name: &String, child_type: &str, child_name: &String, child: Option<ObjectReference>) -> Event {
    Event {
        type_: EventType::Normal,
        reason: format!("Reconciling `{}` {}", src_name, src_type),
        note: Some(format!("Creating `{}` {} for `{}` {}", child_name, child_type, src_name, src_type)),
        action: format!("Creating `{}` {}", child_name, child_type),
        secondary: child,
    }
}

#[must_use] pub fn from_update(src_type: &str, src_name: &String, child_type: &str, child_name: &String, child: Option<ObjectReference>) -> Event {
    Event {
        type_: EventType::Normal,
        reason: format!("Reconciling `{}` {}", src_name, src_type),
        note: Some(format!("Updating `{}` {} for `{}` {}", child_name, child_type, src_name, src_type)),
        action: format!("Updating `{}` {}", child_name, child_type),
        secondary: child,
    }
}

#[must_use] pub fn from_delete(src_type: &str, src_name: &String, child_type: &str, child_name: &String, child: Option<ObjectReference>) -> Event {
    Event {
        type_: EventType::Normal,
        reason: format!("Deleting `{}` {}", src_name, src_type),
        note: Some(format!("Deleting `{}` {} for `{}` {}", child_name, child_type, src_name, src_type)),
        action: format!("Deleting `{}` {}", child_name, child_type),
        secondary: child,
    }
}
