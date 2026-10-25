//! The merge conflict resolver takes the conversation's place (the
//! desktop's `merge-conflict` surface) with the code column folded away
//! while it shows; what it changes in git has "Mudanças" read again.

use gpui::{AppContext, Context, Entity};

use super::Shell;
use crate::screens::merge::{MergeResolver, MergeResolverEvent, MergeResolverRequest};

impl Shell {
    pub(super) fn open_merge_resolver(
        &mut self,
        request: MergeResolverRequest,
        cx: &mut Context<Self>,
    ) {
        let resolver = cx.new(|cx| MergeResolver::new(request, cx));
        let events = cx.subscribe(&resolver, Self::on_merge_event);
        self.merge = Some((resolver, events));
        cx.notify();
    }

    fn on_merge_event(
        &mut self,
        _: Entity<MergeResolver>,
        event: &MergeResolverEvent,
        cx: &mut Context<Self>,
    ) {
        match event {
            MergeResolverEvent::Closed => {
                self.merge = None;
                cx.notify();
            }
            MergeResolverEvent::StateChanged => {
                self.code_column
                    .update(cx, |column, cx| column.refresh_git(cx));
            }
        }
    }

    /// The resolver open over the conversation.
    pub(super) fn merge_resolver(&self) -> Option<Entity<MergeResolver>> {
        self.merge.as_ref().map(|(resolver, _)| resolver.clone())
    }
}
