//! agentview: browse AI coding-agent session transcripts (Claude Code's
//! `~/.claude/projects/*/*.jsonl`) — sessions in the sidebar, a virtualized
//! timeline of turns and tool calls, a detail pane, token totals.
//!
//! `cargo run --release` opens the viewer; `cargo run -- shot` renders a
//! deterministic screenshot from the bundled fixture to
//! `gallery/agentview.png` (headlessly — this app is its own demo of
//! fenestra's verify-by-rendering loop).

mod parse;

use std::path::PathBuf;
use std::rc::Rc;

use fenestra::prelude::*;
use parse::{Event, Role, parse_session};

struct Session {
    path: PathBuf,
    label: String,
    when: String,
}

struct Viewer {
    sessions: Rc<Vec<Session>>,
    selected: Option<usize>,
    events: Rc<Vec<Event>>,
    detail: Option<usize>,
    filter: String,
    dark: bool,
    status: String,
    proxy: Option<Proxy<Msg>>,
}

#[derive(Clone)]
enum Msg {
    Pick(usize),
    Loaded(Vec<Event>),
    Failed(String),
    Detail(usize),
    Filter(String),
    Dark(bool),
}

impl Viewer {
    fn new(sessions: Vec<Session>) -> Self {
        Self {
            sessions: Rc::new(sessions),
            selected: None,
            events: Rc::new(Vec::new()),
            detail: None,
            filter: String::new(),
            dark: true,
            status: String::new(),
            proxy: None,
        }
    }

    fn with_events(mut self, events: Vec<Event>) -> Self {
        self.selected = Some(0);
        self.detail = Some(1.min(events.len().saturating_sub(1)));
        self.events = Rc::new(events);
        self
    }
}

impl App for Viewer {
    type Msg = Msg;

    fn init(&mut self, proxy: Proxy<Msg>) {
        self.proxy = Some(proxy);
    }

    fn update(&mut self, msg: Msg) {
        match msg {
            Msg::Pick(i) => {
                let Some(session) = self.sessions.get(i) else {
                    return;
                };
                self.selected = Some(i);
                self.detail = None;
                self.status = "loading…".to_owned();
                let Some(proxy) = self.proxy.clone() else {
                    return;
                };
                let path = session.path.clone();
                std::thread::spawn(move || match std::fs::read_to_string(&path) {
                    Ok(text) => proxy.send(Msg::Loaded(parse_session(&text))),
                    Err(e) => proxy.send(Msg::Failed(e.to_string())),
                });
            }
            Msg::Loaded(events) => {
                self.status = format!("{} events", events.len());
                self.events = Rc::new(events);
            }
            Msg::Failed(e) => self.status = format!("error: {e}"),
            Msg::Detail(i) => self.detail = Some(i),
            Msg::Filter(s) => {
                self.filter = s;
                self.detail = None;
            }
            Msg::Dark(d) => self.dark = d,
        }
    }

    fn theme(&self) -> Theme {
        if self.dark {
            Theme::dark()
        } else {
            Theme::light()
        }
    }

    fn view(&self) -> Element<Msg> {
        let theme = self.theme();
        row().w_full().h_full().bg(theme.bg).children([
            self.sidebar(),
            divider().w(1.0).h_full(),
            self.main(),
        ])
    }
}

fn role_status(role: Role) -> Status {
    match role {
        Role::User => Status::Accent,
        Role::Assistant => Status::Success,
        Role::Tool => Status::Warning,
    }
}

impl Viewer {
    fn sidebar(&self) -> Element<Msg> {
        let sessions = Rc::clone(&self.sessions);
        let selected = self.selected;
        col()
            .w(300.0)
            .h_full()
            .themed(|t: &Theme, s| s.bg(t.surface))
            .children([
                row()
                    .items_center()
                    .gap(SP2)
                    .px(SP4)
                    .h(56.0)
                    .shrink0()
                    .children([
                        div()
                            .w(14.0)
                            .h(14.0)
                            .rounded(4.0)
                            .themed(|t: &Theme, s| s.bg(t.accent)),
                        text("agentview").weight(Weight::Semibold),
                        spacer(),
                        switch(self.dark)
                            .on_toggle(Msg::Dark(!self.dark))
                            .id("dark")
                            .into(),
                    ]),
                divider(),
            ])
            .children([if self.sessions.is_empty() {
                col().p(SP4).children([text("no sessions found")
                    .size(TextSize::Sm)
                    .themed(|t: &Theme, s| s.color(t.text_subtle))])
            } else {
                virtual_list(self.sessions.len(), 56.0, move |i| {
                    let session = &sessions[i];
                    let active = selected == Some(i);
                    let mut item = col()
                        .justify_center()
                        .gap(SP0)
                        .px(SP4)
                        .cursor(Cursor::Pointer)
                        .on_click(Msg::Pick(i))
                        .semantics(Semantics::Button)
                        .label(session.label.clone())
                        .children([
                            text(&session.label).size(TextSize::Sm).truncate().themed(
                                move |t: &Theme, s| {
                                    if active {
                                        s.color(t.accent_text)
                                    } else {
                                        s.color(t.text)
                                    }
                                },
                            ),
                            text(&session.when)
                                .size(TextSize::Xs)
                                .mono()
                                .themed(|t: &Theme, s| s.color(t.text_subtle)),
                        ]);
                    if active {
                        item = item.themed(|t: &Theme, s| s.bg(t.accent_bg));
                    } else {
                        item = item.hover_themed(|t, s| s.bg(t.neutrals.step(3)));
                    }
                    item
                })
                .id("sessions")
            }])
    }

    fn main(&self) -> Element<Msg> {
        let Some(_) = self.selected else {
            return col()
                .grow()
                .h_full()
                .items_center()
                .justify_center()
                .gap(SP2)
                .children([
                    text("Pick a session").weight(Weight::Semibold),
                    text("Transcripts from ~/.claude/projects appear on the left.")
                        .size(TextSize::Sm)
                        .themed(|t: &Theme, s| s.color(t.text_muted)),
                ]);
        };

        let needle = self.filter.to_lowercase();
        let visible: Rc<Vec<usize>> = Rc::new(
            self.events
                .iter()
                .enumerate()
                .filter(|(_, e)| {
                    needle.is_empty()
                        || e.preview.to_lowercase().contains(&needle)
                        || e.tool
                            .as_deref()
                            .is_some_and(|t| t.to_lowercase().contains(&needle))
                })
                .map(|(i, _)| i)
                .collect(),
        );
        let tokens_in: u64 = self.events.iter().map(|e| e.tokens_in).sum();
        let tokens_out: u64 = self.events.iter().map(|e| e.tokens_out).sum();

        col().grow().h_full().children([
            row()
                .items_center()
                .gap(SP3)
                .px(SP5)
                .h(56.0)
                .shrink0()
                .children([
                    text(format!("{} of {} events", visible.len(), self.events.len()))
                        .size(TextSize::Sm)
                        .weight(Weight::Medium),
                    badge(format!("in {tokens_in}"), Status::Accent),
                    badge(format!("out {tokens_out}"), Status::Success),
                    spacer(),
                    text_input(&self.filter)
                        .placeholder("Filter events…")
                        .width(220.0)
                        .on_input(Msg::Filter)
                        .id("filter")
                        .into(),
                ]),
            divider(),
            row().grow().children([
                self.timeline(&visible),
                divider().w(1.0).h_full(),
                self.detail_pane(),
            ]),
        ])
    }

    fn timeline(&self, visible: &Rc<Vec<usize>>) -> Element<Msg> {
        let events = Rc::clone(&self.events);
        let visible = Rc::clone(visible);
        let detail = self.detail;
        col()
            .grow()
            .h_full()
            .children([virtual_list(visible.len(), 44.0, move |row_index| {
                let i = visible[row_index];
                let event = &events[i];
                let active = detail == Some(i);
                let mut item = row()
                    .items_center()
                    .gap(SP3)
                    .px(SP4)
                    .cursor(Cursor::Pointer)
                    .on_click(Msg::Detail(i))
                    .children([
                        badge(event.role.label(), role_status(event.role)),
                        text(&event.time)
                            .size(TextSize::Xs)
                            .mono()
                            .themed(|t: &Theme, s| s.color(t.text_subtle)),
                    ])
                    .children(
                        event
                            .tool
                            .as_ref()
                            .map(|t| badge(t.as_str(), Status::Warning))
                            .into_iter()
                            .collect::<Vec<_>>(),
                    )
                    .children([text(&event.preview)
                        .size(TextSize::Sm)
                        .truncate()
                        .grow()
                        .themed(|t: &Theme, s| s.color(t.text))]);
                if active {
                    item = item.themed(|t: &Theme, s| s.bg(t.accent_bg));
                } else {
                    item = item.hover_themed(|t, s| s.bg(t.neutrals.step(2)));
                }
                item
            })
            .id("timeline")])
    }

    fn detail_pane(&self) -> Element<Msg> {
        let body: Element<Msg> = match self.detail.and_then(|i| self.events.get(i)) {
            None => text("Select an event")
                .size(TextSize::Sm)
                .themed(|t: &Theme, s| s.color(t.text_subtle)),
            Some(event) => col().gap(SP3).children([
                row().items_center().gap(SP2).children([
                    badge(event.role.label(), role_status(event.role)),
                    text(&event.time)
                        .size(TextSize::Xs)
                        .mono()
                        .themed(|t: &Theme, s| s.color(t.text_subtle)),
                ]),
                text(if event.detail.is_empty() {
                    "(no content)"
                } else {
                    &event.detail
                })
                .size(TextSize::Sm)
                .mono()
                .leading(1.55)
                .themed(|t: &Theme, s| s.color(t.text_muted)),
            ]),
        };
        col()
            .w(460.0)
            .h_full()
            .p(SP4)
            .scroll_y()
            .id("detail")
            .children([body])
    }
}

fn scan_sessions() -> Vec<Session> {
    let home = std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from);
    let Some(root) = home.map(|h| h.join(".claude/projects")) else {
        return Vec::new();
    };
    let mut sessions: Vec<(std::time::SystemTime, Session)> = Vec::new();
    let Ok(projects) = std::fs::read_dir(&root) else {
        return Vec::new();
    };
    for project in projects.flatten() {
        let project_name = project.file_name().to_string_lossy().into_owned();
        let short = project_name.rsplit('-').next().unwrap_or("?").to_owned();
        let Ok(files) = std::fs::read_dir(project.path()) else {
            continue;
        };
        for file in files.flatten() {
            let path = file.path();
            if path.extension().is_none_or(|e| e != "jsonl") {
                continue;
            }
            let Ok(meta) = file.metadata() else { continue };
            let modified = meta.modified().unwrap_or(std::time::UNIX_EPOCH);
            let stem = path
                .file_stem()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_default();
            let kb = meta.len() / 1024;
            sessions.push((
                modified,
                Session {
                    path: path.clone(),
                    label: format!("{short} · {}", &stem[..8.min(stem.len())]),
                    when: format!("{kb} KB"),
                },
            ));
        }
    }
    sessions.sort_by_key(|(modified, _)| std::cmp::Reverse(*modified));
    sessions.into_iter().take(300).map(|(_, s)| s).collect()
}

const FIXTURE: &str = include_str!("../tests/fixture.jsonl");

fn main() {
    if std::env::args().any(|a| a == "shot") {
        let viewer = Viewer::new(vec![Session {
            path: PathBuf::new(),
            label: "fenestra · 0a1b2c3d".to_owned(),
            when: "412 KB".to_owned(),
        }])
        .with_events(parse_session(FIXTURE));
        let theme = viewer.theme();
        let image = fenestra::shell::render_element(viewer.view(), &theme, (1280, 760));
        std::fs::create_dir_all("gallery").expect("create gallery dir");
        image.save("gallery/agentview.png").expect("write png");
        println!("wrote gallery/agentview.png");
        return;
    }
    fenestra::run(
        Viewer::new(scan_sessions()),
        WindowOptions::titled("agentview").with_size(1280.0, 760.0),
    )
}
