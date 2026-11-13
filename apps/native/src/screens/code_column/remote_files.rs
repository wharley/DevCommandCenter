//! "Arquivos" of a task that runs on another machine: the tree the local
//! one draws, built from the machine's own listing of the task's checkout.
//! Read when the task opens and when a turn of it ends — no timer. Nothing
//! is read from this computer's disk: the task's folder is not here.

use std::collections::HashSet;
use std::sync::Arc;

use gpui::Context;

use super::{CodeColumn, Files, Loaded};
use crate::data::code_tree::build_code_tree;
use crate::data::remote_files::RemoteTree;

/// The machine's listing as the tree reads a local one: tracked first,
/// then the ignored files, which draw dimmed.
pub(super) fn files_of(tree: RemoteTree) -> Files {
    let ignored: HashSet<String> = tree.ignored.iter().cloned().collect();
    let mut paths = tree.tracked;
    paths.extend(tree.ignored);
    Files { paths, ignored }
}

impl CodeColumn {
    /// The folder whose tree shows: this computer's checkout, or the
    /// machine's (as its mirror stores it) for a task that runs elsewhere.
    pub(crate) fn tree_root(&self) -> Option<String> {
        self.root.clone().or_else(|| self.machine_root.clone())
    }

    pub(super) fn load_remote_files(&mut self, cx: &mut Context<Self>) {
        let Some((api, task)) = self.remote_api() else {
            return;
        };
        self._files = Some(cx.spawn(async move |this, cx| {
            let asked = task.clone();
            let listed = cx
                .background_executor()
                .spawn(async move { api.remote_tree(&asked) })
                .await;
            let _ = this.update(cx, |column, cx| {
                // Another task was opened meanwhile.
                if column.remote.task.as_deref() != Some(task.as_str()) {
                    return;
                }
                column.files = match listed {
                    Ok(tree) => {
                        let files = files_of(tree);
                        column.tree = Arc::new(build_code_tree(&files.paths));
                        Loaded::Ready(files)
                    }
                    // What was read before stays when a later read fails.
                    Err(_) if matches!(column.files, Loaded::Ready(_)) => return,
                    Err(error) => Loaded::Failed(super::remote::reason(error)),
                };
                column.sync_rows();
                cx.notify();
            });
        }));
    }
}

#[cfg(test)]
mod tests {
    use gpui::{TestAppContext, WindowAppearance, px, size};

    use super::*;
    use crate::data::desktop_api::DesktopApi;
    use crate::screens::code_column::CodeColumnTab;
    use crate::test_support::fake_desktop_routes;
    use crate::theme::{Theme, ThemePreference};

    const TREE: &str = "/api/v1/machines/m-1/proxy/api/v1/mobile/workspaces/w-1/tree";

    #[gpui::test]
    fn a_task_on_a_machine_shows_its_files_from_the_machines_listing(cx: &mut TestAppContext) {
        cx.update(|cx| {
            cx.set_global(Theme::new(ThemePreference::Light, WindowAppearance::Light));
            crate::i18n::I18n::init(crate::i18n::Locale::PtBr, cx);
        });
        let listing = r#"{"tracked":["Cargo.toml","src/main.rs"],"ignored":[".env"]}"#;
        let (port, desktop) = fake_desktop_routes(vec![(TREE, 200, listing.into())]);
        let (column, cx) = cx.add_window_view(|_, cx| CodeColumn::new(CodeColumnTab::Files, cx));
        cx.simulate_resize(size(px(380.0), px(700.0)));
        column.update(cx, |column, cx| {
            column.endpoint = Some(DesktopApi::new(port, "k"));
            column.set_root(Some("dcc-machine://m-1/srv/app".into()), cx);
            column.set_remote_task(Some("m-1".into()), Some("w-1".into()), cx);
        });
        cx.run_until_parked();
        desktop.join().unwrap();
        cx.run_until_parked();
        column.read_with(cx, |column, _| {
            // Nothing of this computer is read, and the tree has a folder to key on.
            assert!(column.root.is_none());
            assert_eq!(
                column.tree_root().as_deref(),
                Some("dcc-machine://m-1/srv/app")
            );
            let Loaded::Ready(files) = &column.files else {
                panic!("the machine's files: {:?}", column.files);
            };
            assert_eq!(files.paths.len(), 3);
            assert!(files.ignored.contains(".env"));
            // The folder closed, and the two files at the top.
            let labels: Vec<&str> = column.rows.iter().map(|row| row.label.as_str()).collect();
            assert!(labels.contains(&"src") && labels.contains(&"Cargo.toml"));
        });
        assert!(cx.debug_bounds("code-tree").is_some());
        // Its folder opens like a local one's.
        column.update(cx, |column, cx| column.toggle_folder("src", cx));
        column.read_with(cx, |column, _| {
            assert!(column.rows.iter().any(|row| row.path == "src/main.rs"));
        });
    }

    #[test]
    fn the_machines_listing_reads_as_a_local_one() {
        let files = files_of(RemoteTree {
            tracked: vec!["src/main.rs".into(), "Cargo.toml".into()],
            ignored: vec![".env".into()],
        });
        assert_eq!(files.paths, ["src/main.rs", "Cargo.toml", ".env"]);
        assert!(files.ignored.contains(".env") && files.ignored.len() == 1);
    }
}
