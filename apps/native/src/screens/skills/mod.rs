//! Skills (`SkillsDialog`): the open task's checkout's skills (one source in
//! `.devcommandcenter/skills/`, compiled to each agent), the context its
//! agents already get, and the bundled catalog; a skill made, edited,
//! taken from the catalog or imported from GitHub, then compiled.

mod form;
mod library;
#[cfg(test)]
mod tests;
mod view;

use dcc_infra::skills::{SkillImportPreview, SkillRecord};
use dcc_infra::skills_context::SkillContextDetection;
use gpui::{AppContext, Context, Entity, EventEmitter, Subscription, Task, Window};

use crate::data::desktop_api::{ApiError, DesktopApi};
use crate::data::skills::valid_name;
use crate::i18n::{Args, Translate};
use crate::ui::Toaster;
use crate::ui::text_area::TextArea;
use crate::ui::text_input::{TextInput, TextInputEvent};

pub enum SkillsEvent {
    Closed,
    /// Saved, imported or deleted: the sidebar recounts.
    Changed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Section {
    Library,
    Detected,
    Catalog,
}

/// `FormState`: one skill being written.
pub(super) struct Form {
    editing_existing: bool,
    from_catalog: bool,
    github: Option<SkillImportPreview>,
    source_url: Option<String>,
    name: Entity<TextInput>,
    description: Entity<TextInput>,
    body: Entity<TextArea>,
    targets: Vec<String>,
    disable_invocation: bool,
    _events: Vec<Subscription>,
}

pub struct Skills {
    /// The open task's checkout (`resolveActiveSkillsCheckout`).
    root: Option<String>,
    /// The task, which authorises the writes.
    task: Option<String>,
    api: Option<DesktopApi>,
    section: Section,
    search: Entity<TextInput>,
    github_url: Entity<TextInput>,
    skills: Vec<SkillRecord>,
    detections: Vec<SkillContextDetection>,
    loading: bool,
    form: Option<Form>,
    busy: bool,
    _inputs: Vec<Subscription>,
    _work: Option<Task<()>>,
}

impl EventEmitter<SkillsEvent> for Skills {}

/// `skillsErrorMessage`: the backend's words, else the fallback.
fn message(error: &ApiError, fallback: gpui::SharedString) -> String {
    let text = error.to_string();
    if text.trim().is_empty() {
        fallback.to_string()
    } else {
        text
    }
}

impl Skills {
    pub fn new(
        root: Option<String>,
        task: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let t = |key: &str, cx: &Context<Self>| Translate::t(&**cx, key);
        let search_placeholder = t("skills.design.search", cx);
        let url_placeholder = t("skills.github.urlPlaceholder", cx);
        let search = cx.new(|cx| TextInput::new(window, cx).with_placeholder(search_placeholder));
        let github_url = cx.new(|cx| TextInput::new(window, cx).with_placeholder(url_placeholder));
        let inputs = vec![
            cx.subscribe(&search, |skills, _, event, cx| match event {
                TextInputEvent::Changed => cx.notify(),
                TextInputEvent::Cancelled => skills.close(cx),
                _ => {}
            }),
            cx.subscribe(&github_url, |skills, _, event, cx| match event {
                TextInputEvent::Changed => cx.notify(),
                TextInputEvent::Submitted => skills.preview_import(cx),
                TextInputEvent::Cancelled => skills.close(cx),
                _ => {}
            }),
        ];
        let mut skills = Self {
            root,
            task,
            api: if cfg!(test) { None } else { DesktopApi::load() },
            section: Section::Library,
            search,
            github_url,
            skills: Vec::new(),
            detections: Vec::new(),
            loading: false,
            form: None,
            busy: false,
            _inputs: inputs,
            _work: None,
        };
        skills.refresh(cx);
        skills
    }

    #[cfg(test)]
    pub fn set_api(&mut self, api: DesktopApi) {
        self.api = Some(api);
    }

    pub fn search_input(&self) -> Entity<TextInput> {
        self.search.clone()
    }

    fn close(&mut self, cx: &mut Context<Self>) {
        if !self.busy {
            cx.emit(SkillsEvent::Closed);
        }
    }

    /// Writes need the task (`workspaceId`).
    fn can_write(&self) -> bool {
        self.root.is_some() && self.task.is_some()
    }

    fn refresh(&mut self, cx: &mut Context<Self>) {
        let Some(root) = self.root.clone() else {
            self.skills.clear();
            self.detections.clear();
            return;
        };
        self.loading = true;
        cx.notify();
        self._work = Some(cx.spawn(async move |this, cx| {
            let read = cx
                .background_executor()
                .spawn(async move {
                    (
                        dcc_infra::skills::load_skills(&root),
                        dcc_infra::skills_context::detect_context_sources(&root, Some(&root)),
                    )
                })
                .await;
            let _ = this.update(cx, |skills, cx| {
                skills.loading = false;
                (skills.skills, skills.detections) = read;
                cx.notify();
            });
        }));
    }

    fn set_section(&mut self, section: Section, cx: &mut Context<Self>) {
        self.section = section;
        cx.notify();
    }

    fn filtered(&self, cx: &gpui::App) -> Vec<&SkillRecord> {
        let query = self.search.read(cx).text().to_string();
        self.skills
            .iter()
            .filter(|skill| crate::data::skills::matches(skill, &query))
            .collect()
    }

    /// A write on the desktop, then the compile; `done` runs on success.
    fn write(
        &mut self,
        write: impl FnOnce(&DesktopApi, &str, &str) -> Result<(), ApiError> + Send + 'static,
        fallback: &'static str,
        done: impl FnOnce(&mut Self, &mut Context<Self>) + 'static,
        cx: &mut Context<Self>,
    ) {
        let (Some(root), Some(task)) = (self.root.clone(), self.task.clone()) else {
            return;
        };
        let api = self.api.clone();
        self.busy = true;
        cx.notify();
        self._work = Some(cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move {
                    let api = api.ok_or(ApiError::DesktopClosed)?;
                    write(&api, &root, &task)?;
                    api.skills_compile(&root, &task)
                })
                .await;
            let _ = this.update(cx, |skills, cx| {
                skills.busy = false;
                match result {
                    Ok(()) => {
                        done(skills, cx);
                        skills.refresh(cx);
                        cx.emit(SkillsEvent::Changed);
                    }
                    Err(error) => {
                        let fallback = Translate::t(&**cx, fallback);
                        Toaster::error(message(&error, fallback), cx);
                    }
                }
                cx.notify();
            });
        }));
    }

    fn delete(&mut self, name: String, cx: &mut Context<Self>) {
        let wanted = name.clone();
        self.write(
            move |api, root, task| api.skills_delete(root, task, &wanted),
            "skills.toast.deleteError",
            move |_, cx| {
                let text = Translate::t_with(
                    &**cx,
                    "skills.toast.deleted",
                    &Args::default().with("name", name),
                );
                Toaster::success(text, cx);
            },
            cx,
        );
    }

    /// `handleGithubImport`: the folder's skill read from GitHub, then
    /// reviewed in the form before anything is written.
    fn preview_import(&mut self, cx: &mut Context<Self>) {
        let url = self.github_url.read(cx).text().trim().to_string();
        if !self.can_write() || url.is_empty() || self.busy {
            return;
        }
        self.busy = true;
        cx.notify();
        self._work = Some(cx.spawn(async move |this, cx| {
            let read = cx
                .background_executor()
                .spawn(async move { dcc_infra::skills::preview(&url) })
                .await;
            let _ = this.update_in(cx, |skills, window, cx| {
                skills.busy = false;
                match read {
                    Ok(preview) => {
                        skills
                            .github_url
                            .update(cx, |input, cx| input.set_text("", cx));
                        skills.open_import(preview, window, cx);
                    }
                    Err(error) => {
                        let text = if error.trim().is_empty() {
                            Translate::t(&**cx, "skills.github.importError").to_string()
                        } else {
                            error
                        };
                        Toaster::error(text, cx);
                    }
                }
                cx.notify();
            });
        }));
    }

    /// `handleSave`: checked here, then saved (or imported) and compiled.
    fn save(&mut self, cx: &mut Context<Self>) {
        let Some(form) = &self.form else {
            return;
        };
        let name = form.name.read(cx).text().trim().to_string();
        let description = form.description.read(cx).text().trim().to_string();
        let problem = if !valid_name(&name) {
            Some("skills.toast.nameInvalid")
        } else if description.is_empty() {
            Some("skills.toast.descriptionRequired")
        } else if form.targets.is_empty() {
            Some("skills.toast.pickTarget")
        } else {
            None
        };
        if let Some(problem) = problem {
            Toaster::error(Translate::t(&**cx, problem), cx);
            return;
        }
        let skill = SkillRecord {
            name: name.clone(),
            description,
            body: form.body.read(cx).text().to_string(),
            target_agents: form.targets.clone(),
            disable_model_invocation: form.disable_invocation,
            scope: "project".into(),
            source_url: form.source_url.clone(),
        };
        let preview = form.github.clone();
        self.write(
            move |api, root, task| match &preview {
                Some(preview) => api.skills_import(root, task, preview, &skill),
                None => api.skills_save(root, task, &skill),
            },
            "skills.toast.saveError",
            move |skills, cx| {
                let text = Translate::t_with(
                    &**cx,
                    "skills.toast.saved",
                    &Args::default().with("name", name),
                );
                Toaster::success(text, cx);
                skills.form = None;
                skills.section = Section::Library;
                skills.search.update(cx, |input, cx| input.set_text("", cx));
            },
            cx,
        );
    }
}
