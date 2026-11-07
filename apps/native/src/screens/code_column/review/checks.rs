//! "Corrigir CI" on GitHub: the checks that failed, each with its failing
//! step's log, go into the composer as a request to the agent, instead of
//! the person reading them in the browser and pasting them back. A pull
//! request blocked by anything else (a review, a conflict) or on another
//! forge still opens in the browser.

use dcc_core::domain::commit_mode::{CommitButtonStatus, CommitMode};
use gpui::Context;

use super::super::{CodeColumn, CodeColumnEvent};
use crate::i18n::Translate;

impl CodeColumn {
    pub(crate) fn fix_checks(&mut self, cx: &mut Context<Self>) {
        let github =
            self.review.forge.as_ref().is_some_and(|forge| {
                forge.connected() && forge.provider.as_deref() == Some("github")
            });
        let checks_failing = self.delivery.pr.as_ref().is_some_and(|pr| {
            pr.merge_state_status.as_deref() == Some("UNSTABLE") && pr.checks_failing != Some(false)
        });
        let (true, Some(root), Ok(api)) = (
            github && checks_failing,
            self.root.clone(),
            self.review_api(),
        ) else {
            return self.open_request(CommitMode::Fix, cx);
        };
        let login = self.review.login();
        self.delivery.button = Some(CommitButtonStatus::Busy);
        cx.notify();
        cx.spawn(async move |this, cx| {
            let failure = cx
                .background_executor()
                .spawn(async move {
                    let _ = api.pipeline(&root, login.as_deref());
                    api.delivery_failure(&root).ok().flatten()
                })
                .await;
            let _ = this.update(cx, |column, cx| {
                match failure.filter(|failure| failure.operation == "pipeline") {
                    Some(failure) => {
                        let prompt = failure.agent_prompt();
                        column.review.failure = Some(failure);
                        column.review.failure_open = true;
                        column.settle_button(CommitButtonStatus::Done, cx);
                        cx.emit(CodeColumnEvent::AskAgent {
                            prompt,
                            delivered: cx.t("inspector.deliveryFailure.sentToAgent"),
                        });
                    }
                    None => column.open_request(CommitMode::Fix, cx),
                }
            });
        })
        .detach();
    }
}
