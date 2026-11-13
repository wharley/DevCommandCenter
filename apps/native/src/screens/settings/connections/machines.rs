//! "Máquinas" in "Conexões": the other DCCs this one runs tasks on. A link
//! copied from a machine's "Parear pelo relay" is pasted here; the machine
//! asks its person to approve this computer, and then it is listed with
//! whether it answers now, its system and version, and "Remover".

use std::collections::HashMap;

use gpui::{AnyElement, Context, FontWeight, SharedString, div, prelude::*, px};

use super::super::Settings;
use super::message;
use super::view::{card, icon_label};
use crate::data::machines::{Machine, MachineCard};
use crate::theme::{ActiveTheme, StateTone};
use crate::ui::{Button, ButtonSize, ButtonVariant, Icon, Toaster, spinner};

/// What is known of a machine since the section opened.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum Reach {
    Asking,
    Online(MachineCard),
    Offline,
}

#[derive(Default)]
pub(in crate::screens::settings) struct Machines {
    pub(super) list: Vec<Machine>,
    pub(super) reach: HashMap<String, Reach>,
    /// Waiting for the person on the machine to approve this computer.
    pub(super) pairing: bool,
    pub(super) removing: Option<String>,
}

impl Settings {
    /// The machines, then each one asked whether it answers.
    pub(super) fn load_machines(&mut self, cx: &mut Context<Self>) {
        let Some(api) = self.general.api() else {
            return;
        };
        cx.spawn(async move |this, cx| {
            let read = cx
                .background_executor()
                .spawn(async move { api.machines() })
                .await;
            let _ = this.update(cx, |settings, cx| {
                if let Ok(list) = read {
                    settings.connections.machines.list = list;
                    settings.ask_machines(cx);
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn ask_machines(&mut self, cx: &mut Context<Self>) {
        let Some(api) = self.general.api() else {
            return;
        };
        let ids: Vec<String> = self
            .connections
            .machines
            .list
            .iter()
            .map(|machine| machine.id.clone())
            .collect();
        for id in ids {
            self.connections
                .machines
                .reach
                .insert(id.clone(), Reach::Asking);
            let api = api.clone();
            cx.spawn(async move |this, cx| {
                let asked = id.clone();
                let card = cx
                    .background_executor()
                    .spawn(async move { api.machine_card(&asked) })
                    .await;
                let _ = this.update(cx, |settings, cx| {
                    let reach = match card {
                        Ok(card) => Reach::Online(card),
                        Err(_) => Reach::Offline,
                    };
                    settings.connections.machines.reach.insert(id, reach);
                    cx.notify();
                });
            })
            .detach();
        }
    }

    /// "Adicionar máquina": pairs from the pasted link.
    pub(super) fn add_machine(&mut self, cx: &mut Context<Self>) {
        let link = self
            .connections
            .machine_link
            .read(cx)
            .text()
            .trim()
            .to_string();
        let Some(api) = self.general.api() else {
            return;
        };
        if link.is_empty() || self.connections.machines.pairing {
            return;
        }
        self.connections.machines.pairing = true;
        cx.notify();
        cx.spawn(async move |this, cx| {
            let paired = cx
                .background_executor()
                .spawn(async move { api.pair_machine(&link) })
                .await;
            let _ = this.update(cx, |settings, cx| {
                settings.connections.machines.pairing = false;
                match paired {
                    Ok(machine) => {
                        let field = settings.connections.machine_link.clone();
                        field.update(cx, |input, cx| input.set_text(String::new(), cx));
                        Toaster::success(format!("{} pareada", machine.name), cx);
                        settings.load_machines(cx);
                    }
                    Err(error) => {
                        Toaster::error(message(error, "Não foi possível parear com a máquina"), cx)
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn remove_machine(&mut self, id: String, cx: &mut Context<Self>) {
        let Some(api) = self.general.api() else {
            return;
        };
        self.connections.machines.removing = Some(id.clone());
        cx.notify();
        cx.spawn(async move |this, cx| {
            let asked = id.clone();
            let removed = cx
                .background_executor()
                .spawn(async move { api.remove_machine(&asked) })
                .await;
            let _ = this.update(cx, |settings, cx| {
                settings.connections.machines.removing = None;
                match removed {
                    Ok(_) => {
                        settings.connections.machines.list.retain(|m| m.id != id);
                        settings.connections.machines.reach.remove(&id);
                    }
                    Err(error) => Toaster::error(message(error, "Falha ao remover a máquina"), cx),
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn machine_row(&self, machine: &Machine, cx: &mut Context<Self>) -> AnyElement {
        let theme = cx.theme().clone();
        let colors = &theme.colors;
        let reach = self.connections.machines.reach.get(&machine.id);
        let (dot, state): (gpui::Hsla, SharedString) = match reach {
            Some(Reach::Online(card)) => (
                StateTone::Done.text(&theme),
                format!("online · {}", card.label()).into(),
            ),
            Some(Reach::Offline) => (colors.muted_foreground.opacity(0.5), "offline".into()),
            _ => (colors.muted_foreground.opacity(0.5), "conferindo…".into()),
        };
        // The machine's own name once it answered; the listed one until then.
        let name = match reach {
            Some(Reach::Online(card)) if !card.name.is_empty() => card.name.clone(),
            _ => machine.name.clone(),
        };
        let id = machine.id.clone();
        let removing = self.connections.machines.removing.as_deref() == Some(id.as_str());
        div()
            .debug_selector({
                let id = id.clone();
                move || format!("settings-machine-{id}")
            })
            .flex()
            .items_center()
            .gap(px(10.0))
            .px(px(16.0))
            .py(px(10.0))
            .border_t_1()
            .border_color(colors.border.opacity(0.6))
            .child(div().flex_none().size(px(8.0)).rounded_full().bg(dot))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .child(
                        div()
                            .truncate()
                            .text_size(px(13.0))
                            .font_weight(FontWeight::MEDIUM)
                            .child(name),
                    )
                    .child(
                        div()
                            .truncate()
                            .text_size(px(11.0))
                            .text_color(colors.muted_foreground)
                            .child(format!("{state} · {}", machine.fingerprint)),
                    ),
            )
            .child(
                Button::new(SharedString::from(format!("settings-machine-remove-{id}")))
                    .variant(ButtonVariant::Ghost)
                    .size(ButtonSize::Sm)
                    .text_size(12.0)
                    .disabled(removing)
                    .on_click(cx.listener(move |settings, _, _, cx| {
                        settings.remove_machine(id.clone(), cx)
                    }))
                    .child("Remover"),
            )
            .into_any_element()
    }

    pub(super) fn machines_card(&self, cx: &mut Context<Self>) -> AnyElement {
        let colors = cx.theme().colors.clone();
        let machines = &self.connections.machines;
        let empty = self
            .connections
            .machine_link
            .read(cx)
            .text()
            .trim()
            .is_empty();
        let rows: Vec<AnyElement> = machines
            .list
            .iter()
            .map(|machine| self.machine_row(machine, cx))
            .collect();
        card(cx)
            .debug_selector(|| "settings-machines".into())
            .child(
                div()
                    .p(px(16.0))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.0))
                            .text_size(px(14.0))
                            .font_weight(FontWeight::MEDIUM)
                            .child(Icon::new("server").size(16.0))
                            .child("Máquinas"),
                    )
                    .child(
                        div()
                            .mt(px(6.0))
                            .text_size(px(12.0))
                            .line_height(px(18.0))
                            .text_color(colors.muted_foreground)
                            .child(
                                "Rode tarefas em outro DCC, como o servidor de casa. Na outra \
                                 máquina, abra Conexões › Parear pelo relay e copie o link; cole \
                                 aqui e aprove lá. As tarefas usam as contas e os projetos dela.",
                            ),
                    )
                    .child(
                        div()
                            .mt(px(12.0))
                            .flex()
                            .items_center()
                            .gap(px(8.0))
                            .child(
                                div()
                                    .debug_selector(|| "settings-machine-link".into())
                                    .flex_1()
                                    .min_w_0()
                                    .child(self.connections.machine_link.clone()),
                            )
                            .child(
                                Button::new("settings-machine-add")
                                    .size(ButtonSize::Sm)
                                    .text_size(12.0)
                                    .disabled(empty || machines.pairing)
                                    .on_click(
                                        cx.listener(|settings, _, _, cx| settings.add_machine(cx)),
                                    )
                                    .child(if machines.pairing {
                                        icon_label(
                                            spinner("settings-machine-pairing", 14.0)
                                                .into_any_element(),
                                            "Aguardando aprovação na máquina…",
                                        )
                                    } else {
                                        icon_label(
                                            Icon::new("plus").size(14.0).into_any_element(),
                                            "Adicionar máquina",
                                        )
                                    }),
                            ),
                    ),
            )
            .children(rows)
            .into_any_element()
    }
}
