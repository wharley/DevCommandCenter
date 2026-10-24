//! `collectPendingPermissionRequests`: the permission requests still waiting
//! on the person, oldest first, each once. The desktop lifts them out of the
//! thread into the panel above the composer.

use super::{Annotation, TimelineMessage};

pub fn pending_permission_requests(messages: &[TimelineMessage]) -> Vec<&Annotation> {
    let mut seen: Vec<&str> = Vec::new();
    let mut pending = Vec::new();
    for message in messages {
        for annotation in &message.annotations {
            let Annotation::Approval {
                id,
                streaming: true,
                behavior,
                ..
            } = annotation
            else {
                continue;
            };
            if behavior
                .as_deref()
                .is_some_and(|behavior| !behavior.is_empty())
                || seen.contains(&id.as_str())
            {
                continue;
            }
            seen.push(id);
            pending.push(annotation);
        }
    }
    pending
}
