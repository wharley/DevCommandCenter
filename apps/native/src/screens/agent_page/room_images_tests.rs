//! Images in the team room, headlessly: a chip in the field goes to the
//! agent as `@/path.png`, and the person's line shows it as a thumbnail
//! under their words.

use gpui::{AppContext, TestAppContext, WindowAppearance};

use super::AgentPage;
use crate::data::agents::{AgentRecord, AgentSession, AgentState};
use crate::data::loader::SidebarData;
use crate::data::team_room::{HomeFeed, RoomLine, Speaker};
use crate::screens::composer::Composer;
use crate::screens::sidebar::Sidebar;
use crate::theme::{Theme, ThemePreference};
use crate::ui::text_area::Chip;

fn reviewer() -> AgentRecord {
    AgentRecord {
        id: "rev".into(),
        name: "Revisor".into(),
        preset: Some("reviewer".into()),
        avatar: dcc_core::domain::agent::AgentAvatar {
            color: "blue".into(),
            arms: 3,
            eyes: "round".into(),
            shape: "octopus".into(),
            accessory: "none".into(),
        },
        provider_id: Some("claude_code".into()),
        model: None,
        effort: None,
        kickoff_prompt: String::new(),
        role: "r".into(),
        offer_prompt: String::new(),
        extra_instructions: String::new(),
        projects: Vec::new(),
        monthly_token_limit: None,
        tool_access: Default::default(),
        auto_start_tasks: false,
        may_call: Vec::new(),
        account_id: None,
        sessions: vec![AgentSession {
            session_id: "s-home".into(),
            workspace_id: "w-home".into(),
            state: AgentState::Idle,
            completed_at: None,
            project_id: "dcc-agent-rev".into(),
            title: None,
            updated_at: "2026-11-10T10:00:00Z".into(),
            room: None,
        }],
    }
}

#[gpui::test]
fn an_image_goes_with_the_words_and_shows_in_the_bubble(cx: &mut TestAppContext) {
    cx.update(|cx| {
        cx.set_global(Theme::new(ThemePreference::Light, WindowAppearance::Light));
        crate::i18n::I18n::init(crate::i18n::Locale::PtBr, cx);
        crate::ui::text_input::bind_keys(cx);
    });
    let dir = tempfile::tempdir().unwrap();
    let seen = dir.path().join("seen.json");
    let shot = dir.path().join("dcc_img_1.png");
    image::RgbImage::new(4, 2).save(&shot).unwrap();
    let shot = shot.to_string_lossy().to_string();
    let (page, cx) = cx.add_window_view(move |window, cx| {
        let sidebar = cx.new(|cx| Sidebar::new(false, false, false, seen, cx));
        let composer = cx.new(|cx| Composer::new(window, cx));
        AgentPage::new("rev".into(), sidebar, composer, cx)
    });
    cx.simulate_resize(gpui::size(gpui::px(1200.0), gpui::px(1400.0)));
    page.update(cx, |page, cx| {
        page.sidebar.update(cx, |sidebar, cx| {
            sidebar.apply_for_tests(
                SidebarData {
                    agents: vec![reviewer()],
                    ..SidebarData::default()
                },
                cx,
            )
        });
        page.open_room(None, cx);
    });
    // The room is made when it is first drawn.
    cx.run_until_parked();
    let room = page.update(cx, |page, _| page.room.as_ref().unwrap().0.clone());

    // The field's chip goes as `@path` after the words.
    let chip = shot.clone();
    let sent = room.update(cx, |room, cx| {
        room.input.update(cx, |area, cx| {
            area.set_text("olha esse erro", cx);
            area.insert_chip(Chip::Image(chip), None, cx);
        });
        room.prompt_text(cx)
    });
    assert_eq!(sent, format!("olha esse erro @{shot}"));

    // The person's line: the words in the bubble, the image under them.
    room.update(cx, |room, cx| {
        let mut feed = HomeFeed::new("rev", "s-home");
        feed.lines = vec![RoomLine {
            key: "a".into(),
            agent_id: "rev".into(),
            session_id: "s-home".into(),
            speaker: Speaker::Person,
            text: sent,
            at: "2026-11-10T10:00:00Z".into(),
            receipt: None,
            tasks: Vec::new(),
            steps: Default::default(),
        }];
        room.take(vec![feed]);
        cx.notify();
    });
    cx.run_until_parked();
    assert!(cx.debug_bounds("room-said-0").is_some());
    assert!(cx.debug_bounds("room-said-0-image-0").is_some());
}
