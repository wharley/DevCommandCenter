//! The cards an agent proposes from its conversation — a source to watch, a
//! routine, a reply — show as soon as its turn ends: the room reads the two small
//! tables again then, instead of waiting for their clocks' next minute (a
//! card that took up to a minute left the person wondering what happened).
//! The clocks still decide when a source or a routine runs.

use gpui::Context;

use super::room::TeamRoom;
use crate::data::agent_sources::Sources;
use crate::data::routines::Routines;

impl TeamRoom {
    pub(crate) fn refresh_proposed(&self, cx: &mut Context<Self>) {
        if cfg!(test) {
            return;
        }
        cx.spawn(async move |_, cx| {
            let (sources, routines, replies) = cx
                .background_executor()
                .spawn(async {
                    (
                        crate::data::agent_sources::read(),
                        crate::data::routines::read(),
                        crate::data::agent_replies::read(),
                    )
                })
                .await;
            let _ = cx.update(|cx| {
                cx.set_global(Sources { list: sources });
                cx.set_global(Routines { list: routines });
                cx.set_global(crate::data::agent_replies::Replies { list: replies });
            });
        })
        .detach();
    }
}
