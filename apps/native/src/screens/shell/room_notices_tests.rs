//! The room notices as the shell delivers them: a toast while DCC is in
//! front on another screen, a system notification while it is in the back.

use std::sync::Arc;

use dcc_core::domain::agent_conversation::home_project_id;
use dcc_core::domain::room_notice::RoomNotice;
use gpui::{Entity, TestAppContext, VisualTestContext, WindowAppearance};

use super::Shell;
use crate::data::agents::{AgentRecord, AgentSession, AgentState};
use crate::data::foreground::Foreground;
use crate::data::loader::SidebarData;
use crate::prefs::Prefs;
use crate::theme::{Theme, ThemePreference};
use crate::ui::Toaster;

fn dev(state: AgentState, completed: Option<&str>, at: &str) -> AgentRecord {
    AgentRecord {
        id: "dev".into(),
        name: "Dev".into(),
        preset: None,
        avatar: dcc_core::domain::agent::AgentAvatar {
            shape: "octopus".into(),
            accessory: "none".into(),
            color: "violet".into(),
            arms: 3,
            eyes: "round".into(),
        },
        provider_id: None,
        model: None,
        effort: None,
        kickoff_prompt: String::new(),
        role: String::new(),
        offer_prompt: String::new(),
        extra_instructions: String::new(),
        projects: Vec::new(),
        monthly_token_limit: None,
        tool_access: Default::default(),
        auto_start_tasks: false,
        may_call: Vec::new(),
        account_id: None,
        sessions: vec![AgentSession {
            session_id: "s".into(),
            workspace_id: "w".into(),
            state,
            completed_at: completed.map(str::to_string),
            project_id: home_project_id("dev"),
            title: None,
            updated_at: at.into(),
            room: None,
        }],
    }
}

/// The shell, its sidebar read once, notices read from `reader`.
fn open(cx: &mut TestAppContext, front: bool) -> (Entity<Shell>, &mut VisualTestContext) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("native-ui.json");
    cx.update(|cx| {
        cx.set_global(Theme::new(ThemePreference::Light, WindowAppearance::Light));
        crate::i18n::I18n::init(crate::i18n::Locale::PtBr, cx);
        Toaster::init(cx);
    });
    let (shell, cx) =
        cx.add_window_view(|window, cx| Shell::new(Prefs::default(), path, window, cx));
    cx.run_until_parked();
    cx.update(|_, cx| cx.set_global(Foreground(front)));
    shell.update(cx, |shell, _| {
        shell.set_room_reader(Arc::new(|probe| {
            Some(if probe.waiting {
                RoomNotice::Approval {
                    request_id: "r-1".into(),
                    tool: "Bash".into(),
                    detail: Some("cargo test".into()),
                }
            } else {
                RoomNotice::Done {
                    turn_id: "t-1".into(),
                    first_line: "Troquei o TTL do cache.".into(),
                }
            })
        }))
    });
    (shell, cx)
}

fn read(shell: &Entity<Shell>, agent: AgentRecord, cx: &mut VisualTestContext) {
    let sidebar = shell.read_with(cx, |shell, _| shell.sidebar.clone());
    sidebar.update(cx, |sidebar, cx| {
        sidebar.apply_for_tests(
            SidebarData {
                agents: vec![agent],
                ..Default::default()
            },
            cx,
        )
    });
    cx.run_until_parked();
}

#[gpui::test]
fn an_approval_on_another_screen_is_a_toast(cx: &mut TestAppContext) {
    let (shell, cx) = open(cx, true);
    crate::ui::attention::take_sent();
    read(&shell, dev(AgentState::Working, None, "1"), cx);
    cx.update(|_, cx| {
        assert!(
            Toaster::messages(cx).is_empty(),
            "the first read only records"
        )
    });
    read(&shell, dev(AgentState::NeedsYou, None, "2"), cx);
    cx.update(|_, cx| {
        assert_eq!(
            Toaster::messages(cx),
            vec!["Dev pede aprovação".to_string()]
        )
    });
    assert!(crate::ui::attention::take_sent().is_empty());
}

#[gpui::test]
fn a_finished_turn_in_the_back_is_a_system_notice(cx: &mut TestAppContext) {
    let (shell, cx) = open(cx, false);
    crate::ui::attention::take_sent();
    read(&shell, dev(AgentState::Working, None, "1"), cx);
    read(&shell, dev(AgentState::Done, Some("2"), "2"), cx);
    assert_eq!(
        crate::ui::attention::take_sent(),
        vec![(
            "Dev terminou na Geral".to_string(),
            "Troquei o TTL do cache.".to_string(),
            false
        )]
    );
    cx.update(|_, cx| assert!(Toaster::messages(cx).is_empty()));
}
