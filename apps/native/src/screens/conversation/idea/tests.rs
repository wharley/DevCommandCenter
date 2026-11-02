//! The idea's panel headlessly: it reads `IDEIA.md`, opens its card once the
//! idea is ready, keeps the favourite as the name until the person types, and
//! a name GitHub refuses holds "Publicar".

use gpui::{TestAppContext, WindowAppearance};

use super::IdeaPanel;
use crate::data::idea;
use crate::theme::{Theme, ThemePreference};

const READY: &str = "# Ideia: Salte\n\n## Problema\nAgendar exame é lento.\n\n## Para quem\nClínicas pequenas.\n\n## Referências\n- https://a.dev\n- https://b.dev\n- https://c.dev\n\n## Hipóteses\n- [confirmada] pagam por agenda\n\n## Nome\n- Agenda Já\n- ★ Salte Saúde\n\n## Veredito\nSeguir\n\n## MVP\nAgenda online.\n";

fn open<'a>(
    root: &std::path::Path,
    cx: &'a mut TestAppContext,
) -> (gpui::Entity<IdeaPanel>, &'a mut gpui::VisualTestContext) {
    cx.update(|cx| {
        cx.set_global(Theme::new(ThemePreference::Light, WindowAppearance::Light));
        crate::i18n::I18n::init(crate::i18n::Locale::PtBr, cx);
    });
    let root = root.to_string_lossy().into_owned();
    cx.add_window_view(move |_, _| IdeaPanel::new(root, "idea-project".into(), None))
}

fn read(panel: &gpui::Entity<IdeaPanel>, rows: usize, cx: &mut gpui::VisualTestContext) {
    panel.update(cx, |panel, cx| panel.update_from(false, rows, cx));
    cx.run_until_parked();
    cx.update(|window, _| window.refresh());
    cx.run_until_parked();
}

#[gpui::test]
fn a_ready_idea_opens_its_card_with_the_favourite_as_the_name(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join(idea::IDEA_FILE), "# Ideia: Salte\n").unwrap();
    let (panel, cx) = open(dir.path(), cx);
    read(&panel, 1, cx);
    assert!(cx.debug_bounds("idea-progress").is_some());
    assert!(
        cx.debug_bounds("idea-card").is_none(),
        "a draft keeps it closed"
    );

    std::fs::write(dir.path().join(idea::IDEA_FILE), READY).unwrap();
    read(&panel, 2, cx);
    assert!(
        cx.debug_bounds("idea-card").is_some(),
        "ready: it opens once"
    );
    panel.read_with(cx, |panel, cx| assert_eq!(panel.name(cx), "salte-saude"));

    // Closed by the person, it stays closed on the next read.
    panel.update(cx, |panel, cx| panel.toggle_card(cx));
    read(&panel, 3, cx);
    assert!(cx.debug_bounds("idea-card").is_none());
}

#[gpui::test]
fn a_typed_name_survives_a_new_favourite(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join(idea::IDEA_FILE), READY).unwrap();
    let (panel, cx) = open(dir.path(), cx);
    read(&panel, 1, cx);
    let input = panel.read_with(cx, |panel, _| panel.name.clone().unwrap());

    std::fs::write(
        dir.path().join(idea::IDEA_FILE),
        READY
            .replace("★ Salte Saúde", "Salte Saúde")
            .replace("- Agenda Já", "- ★ Agenda Já"),
    )
    .unwrap();
    read(&panel, 2, cx);
    panel.read_with(cx, |panel, cx| assert_eq!(panel.name(cx), "agenda-ja"));

    input.update(cx, |input, cx| input.set_text("Nome Ruim!", cx));
    panel.update(cx, |panel, _| panel.name_touched = true);
    std::fs::write(dir.path().join(idea::IDEA_FILE), READY).unwrap();
    read(&panel, 3, cx);
    panel.read_with(cx, |panel, cx| {
        assert_eq!(panel.name(cx), "Nome Ruim!");
        assert!(!idea::is_valid_repository_name(&panel.name(cx)));
    });
}
