//! `WorkspaceEditorPicker`: the preferred editor's button (its mark and
//! name) and a chevron to the list, where picking one opens it and makes it
//! the preferred; disabled without a folder.

use dcc_infra::editor_launch::{EDITORS, editor};
use gpui::{AnyElement, Context, div, prelude::*, px};

use crate::i18n::{Args, Translate};
use crate::project::editor::EditorIcon;
use crate::screens::conversation::{ConversationEvent, ConversationView};
use crate::theme::ActiveTheme;
use crate::ui::{
    Button, ButtonSize, ButtonVariant, Icon, Menu, MenuAlign, MenuEntry, MenuItem, Tooltip,
};

impl ConversationView {
    pub(super) fn editor_picker(&self, cx: &mut Context<Self>) -> AnyElement {
        let colors = cx.theme().colors.clone();
        let path = self.data().and_then(|data| data.workspace_path.clone());
        let active = editor(self.header.editor).unwrap_or(&EDITORS[0]);
        let label = if path.is_some() {
            cx.t_with(
                "workbench.openInEditorLabel",
                &Args::default().with("editor", active.label),
            )
        } else {
            cx.t("workbench.openInEditorUnavailable")
        };
        let id = active.id;
        let main = Tooltip::text(
            "header-editor-tip",
            Button::new("header-editor")
                .variant(ButtonVariant::Ghost)
                .size(ButtonSize::Sm)
                .height(32.0)
                .padding_x(8.0)
                .text_size(12.0)
                .muted()
                .disabled(path.is_none())
                .on_click(
                    cx.listener(move |_, _, _, cx| cx.emit(ConversationEvent::OpenInEditor(id))),
                )
                .child(EditorIcon::new(id).size(14.0))
                .child(div().truncate().child(active.label)),
            label,
        );
        let menu_label = cx.t("workbench.openInEditorMenuLabel");
        let view = cx.entity().downgrade();
        let chosen = active.id;
        let disabled = path.is_none();
        let border = colors.border.opacity(0.4);
        let trigger = move |_open: bool| {
            div()
                .border_l_1()
                .border_color(border)
                .child(
                    Button::new("header-editor-menu")
                        .variant(ButtonVariant::Ghost)
                        .size(ButtonSize::Sm)
                        .height(32.0)
                        .padding_x(0.0)
                        .muted()
                        .disabled(disabled)
                        .child(
                            div()
                                .w(px(28.0))
                                .flex()
                                .justify_center()
                                .opacity(0.7)
                                .child(Icon::new("chevron-down").size(14.0)),
                        ),
                )
                .into_any_element()
        };
        let menu = Menu::dropdown("header-editor-list", trigger, move |_, _| {
            let mut entries = vec![MenuEntry::Label(menu_label.clone()), MenuEntry::Separator];
            for option in EDITORS.iter() {
                let view = view.clone();
                let id = option.id;
                let mut item = MenuItem::new(option.label, move |_, cx| {
                    let _ = view.update(cx, |_, cx| cx.emit(ConversationEvent::OpenInEditor(id)));
                })
                .leading(EditorIcon::new(id).size(14.0));
                if id == chosen {
                    item = item.trailing("✓");
                }
                entries.push(item.into());
            }
            entries
        })
        .align(MenuAlign::End)
        .min_width(192.0);
        div()
            .flex()
            .flex_shrink_0()
            .items_center()
            .child(main)
            .child(menu)
            .into_any_element()
    }
}
