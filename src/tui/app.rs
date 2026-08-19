use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::mpsc;
use std::time::Duration;

use anime_launcher_sdk::anime_game_core::sophon::prettify_bytes;
use crossterm::event::{self, Event as TermEvent, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::layout::{Alignment, Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{
    Block, BorderType, Borders, Gauge, List, ListItem, Padding, Paragraph, Tabs, Wrap,
};
use ratatui::{DefaultTerminal, Frame};
use tui_file_explorer::{render as render_file_explorer, ExplorerOutcome, FileExplorer};

use crate::cmd;
use crate::download::{self, Event};
use crate::edition::Edition;
use crate::voice;

const LOG_LIMIT: usize = 100;
const MIN_GAUGE_WIDTH: u16 = 36;
const VOICE_OPTIONS: [(&str, &str); 6] = [
    ("none", "None"),
    ("all", "All voices"),
    ("en-us", "English"),
    ("ja-jp", "Japanese"),
    ("ko-kr", "Korean"),
    ("zh-cn", "Chinese"),
];

#[derive(Clone, Copy, PartialEq, Eq)]
enum Action {
    Download,
    Update,
    Predownload,
    Repair,
    List,
    Quit,
}

const ACTIONS: [(Action, &str); 6] = [
    (Action::Download, "Download game"),
    (Action::Update, "Update game"),
    (Action::Predownload, "Pre-download update"),
    (Action::Repair, "Repair / check files"),
    (Action::List, "List versions"),
    (Action::Quit, "Quit"),
];

struct VoicePicker {
    selected: [bool; VOICE_OPTIONS.len()],
    cursor: usize,
}

impl VoicePicker {
    fn from_value(value: &str) -> Self {
        let requested = voice::parse_arg(Some(value));
        let mut selected = [false; VOICE_OPTIONS.len()];

        if requested.iter().any(|voice| voice == "none") {
            selected[0] = true;
        } else if requested.iter().any(|voice| voice == "all") {
            selected[1] = true;
        } else {
            for (idx, (code, _)) in VOICE_OPTIONS.iter().enumerate().skip(2) {
                selected[idx] = requested.iter().any(|voice| voice == code);
            }
        }

        Self {
            selected,
            cursor: 0,
        }
    }

    fn toggle_current(&mut self) {
        if self.cursor == 0 {
            self.selected[0] = !self.selected[0];
            for selected in &mut self.selected[1..] {
                *selected = false;
            }
        } else if self.cursor == 1 {
            self.selected[1] = !self.selected[1];
            for idx in 0..self.selected.len() {
                if idx != 1 {
                    self.selected[idx] = false;
                }
            }
        } else {
            self.selected[0] = false;
            self.selected[1] = false;
            self.selected[self.cursor] = !self.selected[self.cursor];
        }
    }

    fn value(&self) -> String {
        if self.selected[0] {
            return "none".to_owned();
        }

        if self.selected[1] {
            return "all".to_owned();
        }

        VOICE_OPTIONS
            .iter()
            .enumerate()
            .skip(2)
            .filter(|(idx, _)| self.selected[*idx])
            .map(|(_, (code, _))| *code)
            .collect::<Vec<_>>()
            .join(",")
    }
}

#[derive(PartialEq, Eq)]
enum Screen {
    Menu,
    Params,
    Progress,
    Result,
}

#[derive(Default)]
struct Progress {
    done: u64,
    total: u64,
}

impl Progress {
    fn set(&mut self, done: u64, total: u64) {
        self.done = done;
        self.total = total;
    }

    fn ratio(&self) -> f64 {
        if self.total == 0 {
            0.0
        } else {
            (self.done as f64 / self.total as f64).clamp(0.0, 1.0)
        }
    }

    fn label(&self) -> String {
        format!(
            "{}/{}",
            prettify_bytes(self.done),
            prettify_bytes(self.total)
        )
    }
}

pub fn run(edition: Edition, tracing_rx: mpsc::Receiver<String>) -> anyhow::Result<()> {
    let mut terminal = ratatui::init();
    let result = run_app(&mut terminal, edition, tracing_rx);
    ratatui::restore();
    result
}

fn run_app(
    terminal: &mut DefaultTerminal,
    edition: Edition,
    tracing_rx: mpsc::Receiver<String>,
) -> anyhow::Result<()> {
    let mut app = App::new(edition, tracing_rx);

    while !app.quit {
        app.consume_events();

        terminal.draw(|frame| ui(frame, &mut app))?;

        if event::poll(Duration::from_millis(50))? {
            if let TermEvent::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Press {
                    app.on_key(key);
                }
            }
        }
    }

    Ok(())
}

struct App {
    edition: Edition,
    screen: Screen,
    menu_idx: usize,
    action: Action,
    dest: String,
    threads: String,
    voice: String,
    field_focus: usize,
    phase: String,
    bytes: Progress,
    files: Progress,
    logs: VecDeque<String>,
    tracing_rx: mpsc::Receiver<String>,
    dest_picker: Option<FileExplorer>,
    voice_picker: Option<VoicePicker>,
    rx: Option<mpsc::Receiver<Event>>,
    worker: Option<std::thread::JoinHandle<anyhow::Result<()>>>,
    result: Option<Result<(), String>>,
    list_output: Vec<String>,
    quit: bool,
}

impl App {
    fn new(edition: Edition, tracing_rx: mpsc::Receiver<String>) -> Self {
        Self {
            edition,
            screen: Screen::Menu,
            menu_idx: 0,
            action: Action::Download,
            dest: ".".to_owned(),
            threads: "8".to_owned(),
            voice: String::new(),
            field_focus: 0,
            phase: String::new(),
            bytes: Progress::default(),
            files: Progress::default(),
            logs: VecDeque::new(),
            tracing_rx,
            dest_picker: None,
            voice_picker: None,
            rx: None,
            worker: None,
            result: None,
            list_output: Vec::new(),
            quit: false,
        }
    }

    fn log(&mut self, msg: String) {
        if self.logs.len() >= LOG_LIMIT {
            self.logs.pop_front();
        }
        self.logs.push_back(msg);
    }

    fn on_key(&mut self, key: KeyEvent) {
        match self.screen {
            Screen::Menu => self.menu_key(key),
            Screen::Params => self.params_key(key),
            Screen::Progress => match key.code {
                KeyCode::Char('q') | KeyCode::Char('Q') => self.quit = true,
                _ => {}
            },
            Screen::Result => match key.code {
                KeyCode::Esc | KeyCode::Enter => self.screen = Screen::Menu,
                _ => {}
            },
        }
    }

    fn menu_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Left => self.switch_edition(-1),
            KeyCode::Right => self.switch_edition(1),
            KeyCode::Up => self.menu_idx = (self.menu_idx + ACTIONS.len() - 1) % ACTIONS.len(),
            KeyCode::Down => self.menu_idx = (self.menu_idx + 1) % ACTIONS.len(),
            KeyCode::Enter => {
                self.action = ACTIONS[self.menu_idx].0;
                match self.action {
                    Action::Quit => self.quit = true,
                    Action::List => self.do_list(),
                    _ => {
                        self.field_focus = 0;
                        self.screen = Screen::Params;
                    }
                }
            }
            KeyCode::Esc | KeyCode::Char('q') | KeyCode::Char('Q') => self.quit = true,
            _ => {}
        }
    }

    fn switch_edition(&mut self, delta: i8) {
        let idx = Edition::ALL
            .iter()
            .position(|e| *e == self.edition)
            .unwrap_or(0) as i8;
        let next = (idx + delta).rem_euclid(Edition::ALL.len() as i8) as usize;
        self.edition = Edition::ALL[next];
    }

    fn params_key(&mut self, key: KeyEvent) {
        if self.dest_picker.is_some() {
            self.explorer_key(key);
            return;
        }
        if self.voice_picker.is_some() {
            self.voice_picker_key(key);
            return;
        }

        match key.code {
            KeyCode::Esc => self.screen = Screen::Menu,
            KeyCode::Enter => match self.field_focus {
                0 => self.open_dest_picker(),
                2 => self.open_voice_picker(),
                3 => self.start_worker(),
                _ => {}
            },
            KeyCode::Down | KeyCode::Tab => {
                self.field_focus = (self.field_focus + 1) % 4;
            }
            KeyCode::Up => {
                self.field_focus = (self.field_focus + 3) % 4;
            }
            KeyCode::Backspace if self.field_focus == 1 => {
                self.threads.pop();
            }
            KeyCode::Char(c) if self.field_focus == 1 => {
                self.threads.push(c);
            }
            _ => {}
        }
    }

    fn open_dest_picker(&mut self) {
        let start_dir = picker_start_dir(&self.dest);
        self.dest_picker = Some(FileExplorer::builder(start_dir).show_sizes(false).build());
    }

    fn open_voice_picker(&mut self) {
        self.voice_picker = Some(VoicePicker::from_value(&self.voice));
    }

    fn explorer_key(&mut self, key: KeyEvent) {
        let choose_current = self
            .dest_picker
            .as_ref()
            .is_some_and(|picker| !picker.is_searching())
            && key.code == KeyCode::Char('c')
            && key.modifiers == KeyModifiers::NONE;

        if choose_current {
            if let Some(path) = self
                .dest_picker
                .as_ref()
                .map(|picker| picker.current_dir.clone())
            {
                self.dest = path.to_string_lossy().into_owned();
                self.dest_picker = None;
            }
            return;
        }

        let block_mutation = self
            .dest_picker
            .as_ref()
            .is_some_and(|picker| !picker.is_searching())
            && key.modifiers == KeyModifiers::NONE
            && matches!(
                key.code,
                KeyCode::Char(' ') | KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Char('r')
            );

        if block_mutation {
            return;
        }

        let Some(picker) = self.dest_picker.as_mut() else {
            return;
        };

        match picker.handle_key(key) {
            ExplorerOutcome::Dismissed => self.dest_picker = None,
            ExplorerOutcome::Selected(path) if path.is_dir() => {
                self.dest = path.to_string_lossy().into_owned();
                self.dest_picker = None;
            }
            ExplorerOutcome::Selected(_)
            | ExplorerOutcome::Pending
            | ExplorerOutcome::Unhandled
            | ExplorerOutcome::MkdirCreated(_)
            | ExplorerOutcome::TouchCreated(_)
            | ExplorerOutcome::RenameCompleted(_) => {}
        }
    }

    fn voice_picker_key(&mut self, key: KeyEvent) {
        let Some(picker) = self.voice_picker.as_mut() else {
            return;
        };

        match key.code {
            KeyCode::Up => {
                picker.cursor = (picker.cursor + VOICE_OPTIONS.len() - 1) % VOICE_OPTIONS.len();
            }
            KeyCode::Down => {
                picker.cursor = (picker.cursor + 1) % VOICE_OPTIONS.len();
            }
            KeyCode::Char(' ') if key.modifiers == KeyModifiers::NONE => {
                picker.toggle_current();
                self.voice = picker.value();
            }
            KeyCode::Enter => {
                self.voice = picker.value();
                self.voice_picker = None;
                self.field_focus = 3;
            }
            KeyCode::Esc => self.voice_picker = None,
            _ => {}
        }
    }

    fn do_list(&mut self) {
        self.logs.clear();
        self.result = None;
        self.list_output.clear();

        let mut output = Vec::new();
        let res = cmd::list_text(self.edition, &mut output);

        match res {
            Ok(()) => {
                self.list_output = output;
                self.result = Some(Ok(()));
            }
            Err(err) => {
                self.list_output = vec![format!("ERROR: {err:#}")];
                self.result = Some(Err(err.to_string()));
            }
        }

        self.screen = Screen::Result;
    }

    fn start_worker(&mut self) {
        let edition = self.edition;
        let dest = PathBuf::from(self.dest.trim());
        let threads = self.threads.trim().parse().unwrap_or(8);
        let voices = voice::parse_arg(Some(self.voice.trim()));
        let action = self.action;

        let (tx, rx) = mpsc::channel();

        let handle = std::thread::spawn(move || {
            let no_free_space_check = false;
            let result = match action {
                Action::Download => download::download(
                    edition,
                    dest,
                    threads,
                    None,
                    voices,
                    no_free_space_check,
                    tx.clone(),
                ),
                Action::Update => download::update(
                    edition,
                    dest,
                    threads,
                    None,
                    voices,
                    no_free_space_check,
                    tx.clone(),
                ),
                Action::Predownload => download::pre_download(
                    edition,
                    dest,
                    threads,
                    None,
                    voices,
                    no_free_space_check,
                    tx.clone(),
                ),
                Action::Repair => download::repair(
                    edition,
                    dest,
                    threads,
                    None,
                    voices,
                    no_free_space_check,
                    tx.clone(),
                ),
                _ => unreachable!("cannot start a worker for this action"),
            };

            let _ = tx.send(Event::Finished(
                result.as_ref().map(|_| ()).map_err(|e| e.to_string()),
            ));
            result
        });

        self.rx = Some(rx);
        self.worker = Some(handle);
        self.phase = "Starting".into();
        self.bytes = Progress::default();
        self.files = Progress::default();
        self.logs.clear();
        self.result = None;
        self.screen = Screen::Progress;
    }

    fn consume_events(&mut self) {
        let events: Vec<Event> = self
            .rx
            .as_ref()
            .map(|rx| rx.try_iter().collect())
            .unwrap_or_default();

        for event in events {
            self.handle_event(event);
        }

        let tracing_logs: Vec<String> = self.tracing_rx.try_iter().collect();
        for log in tracing_logs {
            self.log(log);
        }
    }

    fn handle_event(&mut self, event: Event) {
        match event {
            Event::Phase(phase) => self.phase = phase,
            Event::ProgressBytes { downloaded, total } => self.bytes.set(downloaded, total),
            Event::ProgressFiles { downloaded, total } => self.files.set(downloaded, total),
            Event::Message(msg) => self.log(msg),
            Event::Error(err) => self.log(format!("ERROR: {err}")),
            Event::Finished(result) => {
                self.rx = None;
                if let Some(worker) = self.worker.take() {
                    let _ = worker.join();
                }
                self.result = Some(result.clone());
                self.log(match &result {
                    Ok(()) => "Completed successfully".to_owned(),
                    Err(err) => format!("Failed: {err}"),
                });
                self.screen = Screen::Result;
            }
        }
    }
}

fn ui(frame: &mut Frame, app: &mut App) {
    let chunks = Layout::vertical([
        Constraint::Length(3),
        Constraint::Min(0),
        Constraint::Length(3),
    ])
    .split(frame.area());

    render_title(frame, chunks[0], app);
    match app.screen {
        Screen::Menu => render_menu(frame, chunks[1], app),
        Screen::Params => render_params(frame, chunks[1], app),
        Screen::Progress => render_progress(frame, chunks[1], app),
        Screen::Result => render_result(frame, chunks[1], app),
    }
    render_footer(frame, chunks[2], app);
}

fn render_title(frame: &mut Frame, area: Rect, app: &App) {
    let title = Paragraph::new("genshin-dl — Genshin Impact downloader")
        .block(Block::bordered().title(format!(" genshin-dl v{} ", env!("CARGO_PKG_VERSION"))))
        .style(
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )
        .alignment(Alignment::Center);

    frame.render_widget(title, area);

    let tabs = Tabs::new(Edition::ALL.iter().map(|e| e.to_string()))
        .select(
            Edition::ALL
                .iter()
                .position(|e| *e == app.edition)
                .unwrap_or(0),
        )
        .highlight_style(
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        );

    frame.render_widget(
        tabs,
        Rect {
            y: 2,
            height: 1,
            ..area
        },
    );
}

fn render_menu(frame: &mut Frame, area: Rect, app: &App) {
    let lines: Vec<Line> = ACTIONS
        .iter()
        .enumerate()
        .map(|(idx, (action, label))| {
            let selected = idx == app.menu_idx;
            Line::from(Span::styled(
                if selected {
                    format!("> {} {label}", action_icon(*action))
                } else {
                    format!("  {} {label}", action_icon(*action))
                },
                if selected {
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default()
                },
            ))
        })
        .collect();

    let paragraph = Paragraph::new(lines)
        .block(Block::bordered().title(" 🎯 Select an action "))
        .wrap(Wrap { trim: true });

    frame.render_widget(paragraph, area);
}

fn render_params(frame: &mut Frame, area: Rect, app: &mut App) {
    let show_explorer = app.dest_picker.is_some();
    let show_voice = app.voice_picker.is_some();
    let has_middle = show_explorer || show_voice;

    let (action_area, middle_area, params_area) = if has_middle {
        let chunks = Layout::vertical([
            Constraint::Length(3),
            Constraint::Min(0),
            Constraint::Length(11),
        ])
        .split(area);
        (chunks[0], Some(chunks[1]), chunks[2])
    } else {
        let chunks = Layout::vertical([Constraint::Length(3), Constraint::Min(0)]).split(area);
        (chunks[0], None, chunks[1])
    };

    let action = ACTIONS
        .iter()
        .find(|(a, _)| *a == app.action)
        .map(|(_, l)| *l)
        .unwrap_or("");
    let action_para = Paragraph::new(Line::from(Span::styled(
        format!("{} {action}", action_icon(app.action)),
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD),
    )))
    .block(Block::bordered().title(" ⚡ Action "));
    frame.render_widget(action_para, action_area);

    let fields = [
        ("📂 Game directory", app.dest.clone()),
        ("⚙️ Threads", app.threads.clone()),
        (
            "Voices",
            if app.voice.is_empty() {
                "None (action default)".to_owned()
            } else if app.voice == "none" {
                "None".to_owned()
            } else {
                app.voice.clone()
            },
        ),
    ];
    let mut lines = Vec::new();

    for (idx, (label, value)) in fields.iter().enumerate() {
        let focused = idx == app.field_focus;
        let marker = if focused { ">" } else { " " };
        let marker_style = if focused {
            Style::default().fg(Color::Yellow)
        } else {
            Style::default()
        };
        lines.push(Line::from(vec![
            Span::styled(marker, marker_style),
            Span::styled(
                format!(" {label}:"),
                Style::default().add_modifier(Modifier::BOLD),
            ),
        ]));
        lines.push(Line::from(Span::styled(
            format!("    {value}"),
            if focused {
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            },
        )));
        lines.push(Line::from(""));
    }

    lines.push(Line::from(""));
    let start_focused = app.field_focus == 3;
    let start_style = if start_focused {
        Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default()
            .fg(Color::Green)
            .add_modifier(Modifier::BOLD)
    };
    lines.push(Line::from(vec![
        Span::styled(
            if start_focused { ">" } else { " " },
            if start_focused {
                Style::default().fg(Color::Yellow)
            } else {
                Style::default()
            },
        ),
        Span::styled(" ▶ Start", start_style),
    ]));

    let paragraph = Paragraph::new(lines)
        .block(Block::bordered().title(" Parameters "))
        .wrap(Wrap { trim: true });

    frame.render_widget(paragraph, params_area);

    if show_voice {
        if let Some(picker) = app.voice_picker.as_mut() {
            if let Some(middle_area) = middle_area {
                render_voice_picker(frame, middle_area, picker);
            }
        }
    } else if show_explorer {
        if let Some(picker) = app.dest_picker.as_mut() {
            if let Some(middle_area) = middle_area {
                render_file_explorer(picker, frame, middle_area);
            }
        }
    }
}

fn render_voice_picker(frame: &mut Frame, area: Rect, picker: &VoicePicker) {
    let chunks = Layout::vertical([
        Constraint::Length(3),
        Constraint::Min(1),
        Constraint::Length(3),
    ])
    .split(area);

    let summary = if picker.selected[0] {
        "None".to_owned()
    } else if picker.selected[1] {
        "All voices".to_owned()
    } else {
        let codes: Vec<&str> = VOICE_OPTIONS
            .iter()
            .enumerate()
            .skip(2)
            .filter(|(idx, _)| picker.selected[*idx])
            .map(|(_, (code, _))| *code)
            .collect();
        if codes.is_empty() {
            "None".to_owned()
        } else {
            codes.join(", ")
        }
    };
    let selected_count = picker.selected.iter().filter(|selected| **selected).count();

    let header = Paragraph::new(Span::styled(
        summary,
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD),
    ))
    .block(
        Block::default()
            .title(Span::styled(
                " 🎙 Voices ",
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ))
            .title_bottom(
                Line::from(Span::styled(
                    format!(" {selected_count}/{} selected ", VOICE_OPTIONS.len()),
                    Style::default().fg(Color::DarkGray),
                ))
                .right_aligned(),
            )
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(Color::Cyan))
            .padding(Padding::horizontal(1)),
    );
    frame.render_widget(header, chunks[0]);

    let items: Vec<ListItem> = VOICE_OPTIONS
        .iter()
        .enumerate()
        .map(|(idx, (code, label))| {
            let is_cursor = idx == picker.cursor;
            let checked = picker.selected[idx];
            let marker = if checked {
                Span::styled(
                    "◆",
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD),
                )
            } else {
                Span::styled(" ", Style::default())
            };
            let label_style = if is_cursor {
                Style::default().fg(Color::Black)
            } else if checked {
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD)
            };
            let code_style = if is_cursor {
                Style::default().fg(Color::Black)
            } else {
                Style::default().fg(Color::DarkGray)
            };
            let line = Line::from(vec![
                marker,
                Span::styled(format!(" {label} "), label_style),
                Span::styled(format!("({code})"), code_style),
            ]);
            if is_cursor {
                ListItem::new(line).style(
                    Style::default()
                        .bg(Color::Yellow)
                        .fg(Color::Black)
                        .add_modifier(Modifier::BOLD),
                )
            } else {
                ListItem::new(line)
            }
        })
        .collect();

    let pos = format!("{}/{}", picker.cursor + 1, VOICE_OPTIONS.len());
    let list_title = if selected_count > 0 {
        format!(" Voices {pos}  ◆ {selected_count} selected ")
    } else {
        format!(" Voices {pos} ")
    };
    let list = List::new(items).block(
        Block::default()
            .title(Span::styled(
                list_title,
                Style::default().fg(Color::DarkGray),
            ))
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(Color::Cyan)),
    );
    frame.render_widget(list, chunks[1]);

    let footer = Paragraph::new(Span::styled(
        format!(" {selected_count}/{} selected ", VOICE_OPTIONS.len()),
        Style::default().fg(Color::DarkGray),
    ))
    .alignment(Alignment::Right)
    .block(
        Block::default()
            .title(hint_line(&[
                ("↑/↓", "move"),
                ("Space", "toggle"),
                ("Enter", "next"),
                ("Tab", "field"),
                ("Esc", "back"),
            ]))
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(Color::DarkGray)),
    );
    frame.render_widget(footer, chunks[2]);
}

fn picker_start_dir(dest: &str) -> PathBuf {
    let mut path = PathBuf::from(dest.trim());
    if path.as_os_str().is_empty() {
        return std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    }

    while !path.is_dir() {
        if !path.pop() {
            return std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        }
    }

    std::fs::canonicalize(&path).unwrap_or(path)
}

fn render_progress(frame: &mut Frame, area: Rect, app: &mut App) {
    let side_by_side = area.width >= MIN_GAUGE_WIDTH * 2;
    let gauges_height = if side_by_side { 3 } else { 7 };
    let chunks = Layout::vertical([
        Constraint::Length(3),
        Constraint::Length(1),
        Constraint::Length(gauges_height),
        Constraint::Length(1),
        Constraint::Min(0),
    ])
    .split(area);

    let status_style = Style::default()
        .fg(Color::Cyan)
        .add_modifier(Modifier::BOLD);
    let phase = Paragraph::new(Line::from(vec![
        Span::styled(phase_icon(&app.phase), status_style),
        Span::raw(" "),
        Span::styled(app.phase.clone(), status_style),
    ]))
    .block(Block::bordered().title(" ⚙ Status "));
    frame.render_widget(phase, chunks[0]);

    let gauge_chunks = if side_by_side {
        Layout::horizontal([Constraint::Percentage(50), Constraint::Percentage(50)])
            .split(chunks[2])
    } else {
        Layout::vertical([
            Constraint::Length(3),
            Constraint::Length(1),
            Constraint::Length(3),
        ])
        .split(chunks[2])
    };

    let bytes_label = format!("{} ({:.1}%)", app.bytes.label(), app.bytes.ratio() * 100.0);
    let bytes_gauge = Gauge::default()
        .block(Block::bordered().title(" 📦 Bytes "))
        .ratio(app.bytes.ratio())
        .label(Span::styled(
            bytes_label,
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        ))
        .gauge_style(Style::default().fg(Color::Cyan));
    frame.render_widget(bytes_gauge, gauge_chunks[0]);

    let files_label = format!("{} / {} files", app.files.done, app.files.total);
    let files_gauge = Gauge::default()
        .block(Block::bordered().title(" 📁 Files "))
        .ratio(app.files.ratio())
        .label(Span::styled(
            files_label,
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        ))
        .gauge_style(Style::default().fg(Color::Green));
    frame.render_widget(
        files_gauge,
        if side_by_side {
            gauge_chunks[1]
        } else {
            gauge_chunks[2]
        },
    );

    let log_text: Vec<Line> = app.logs.iter().map(|log| render_log_line(log)).collect();

    let log = Paragraph::new(log_text)
        .block(Block::bordered().title(" 📝 Log "))
        .wrap(Wrap { trim: true });

    frame.render_widget(log, chunks[4]);
}

fn phase_icon(phase: &str) -> &'static str {
    if phase.contains("Checking free space") {
        "🔍"
    } else if phase.contains("Checking files") {
        "🔎"
    } else if phase.contains("Pre-downloading") || phase.contains("Downloading") {
        "⬇"
    } else if phase.contains("Patching") || phase.contains("Repairing") {
        "🛠"
    } else if phase.contains("Deleting") {
        "🗑"
    } else if phase.contains("Fetching") {
        "🌐"
    } else if phase.contains("finished") || phase.contains("Already up to date") {
        "✅"
    } else {
        "⚙"
    }
}

fn action_icon(action: Action) -> &'static str {
    match action {
        Action::Download => "⬇",
        Action::Update => "🔄",
        Action::Predownload => "📥",
        Action::Repair => "🛠",
        Action::List => "📋",
        Action::Quit => "🚪",
    }
}

fn render_log_line(log: &str) -> Line<'_> {
    if let Some(message) = log.strip_prefix("ERROR: ") {
        return Line::from(vec![
            Span::styled("ERROR", log_level_style("ERROR")),
            Span::raw(": "),
            Span::raw(message),
        ]);
    }

    let Some((level, rest)) = log.split_once(' ') else {
        return Line::from(Span::raw(log));
    };

    if !matches!(level, "TRACE" | "DEBUG" | "INFO" | "WARN" | "ERROR") {
        return Line::from(Span::raw(log));
    }

    let level = Span::styled(level, log_level_style(level));
    let Some((target, message)) = rest.split_once(": ") else {
        return Line::from(vec![level, Span::raw(" "), Span::raw(rest)]);
    };

    let dim = Style::default().add_modifier(Modifier::DIM);
    Line::from(vec![
        level,
        Span::raw(" "),
        Span::styled(target, dim),
        Span::styled(":", dim),
        Span::raw(" "),
        Span::raw(message),
    ])
}

fn log_level_style(level: &str) -> Style {
    let color = match level {
        "TRACE" => Color::Magenta,
        "DEBUG" => Color::Blue,
        "INFO" => Color::Green,
        "WARN" => Color::Yellow,
        "ERROR" => Color::Red,
        _ => return Style::default(),
    };

    Style::default().fg(color)
}

fn render_result(frame: &mut Frame, area: Rect, app: &mut App) {
    let mut lines: Vec<Line> = Vec::new();

    if !app.list_output.is_empty() {
        lines = app
            .list_output
            .iter()
            .map(|l| Line::from(l.clone()))
            .collect();
    } else {
        match &app.result {
            Some(Ok(())) => lines.push(Line::from(Span::styled(
                "Completed successfully",
                Style::default()
                    .fg(Color::Green)
                    .add_modifier(Modifier::BOLD),
            ))),
            Some(Err(err)) => lines.push(Line::from(Span::styled(
                format!("Failed: {err}"),
                Style::default().fg(Color::Red),
            ))),
            None => lines.push(Line::from("Nothing to show")),
        }
    }

    let paragraph = Paragraph::new(lines)
        .block(Block::bordered().title(" Result "))
        .wrap(Wrap { trim: true });

    frame.render_widget(paragraph, area);
}

fn render_footer(frame: &mut Frame, area: Rect, app: &App) {
    let line: Line = match app.screen {
        Screen::Menu => hint_line(&[
            ("←/→", "edition"),
            ("↑/↓", "select"),
            ("Enter", "confirm"),
            ("q", "quit"),
        ]),
        Screen::Params => {
            if app.dest_picker.is_some() || app.voice_picker.is_some() {
                hint_line(&[
                    ("Esc", "close"),
                    ("↑/↓", "move"),
                    ("Enter", "open"),
                    ("Backspace", "parent"),
                    ("c", "choose current destination"),
                ])
            } else {
                hint_line(&[("↑/↓", "field"), ("Enter", "open options"), ("Esc", "back")])
            }
        }
        Screen::Progress => {
            let mut line = Line::from(Span::styled(
                "Working... ",
                Style::default().fg(Color::DarkGray),
            ));
            line.push_span(Span::styled(
                "q",
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD),
            ));
            line.push_span(Span::styled(
                " quit (worker keeps running)",
                Style::default().fg(Color::DarkGray),
            ));
            line
        }
        Screen::Result => hint_line(&[("Enter/Esc", "back to menu")]),
    };

    let footer = Paragraph::new(line).block(
        Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(Color::White)),
    );
    frame.render_widget(footer, area);
}

fn hint_line<'a>(hints: &[(&'a str, &'a str)]) -> Line<'a> {
    let mut spans = Vec::new();
    for (idx, (key, desc)) in hints.iter().enumerate() {
        if idx > 0 {
            spans.push(Span::raw("   "));
        }
        spans.push(Span::styled(
            *key,
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        ));
        spans.push(Span::raw(" "));
        spans.push(Span::styled(*desc, Style::default().fg(Color::DarkGray)));
    }
    Line::from(spans)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn params_app() -> App {
        let (_, tracing_rx) = mpsc::channel();
        let mut app = App::new(Edition::Global, tracing_rx);
        app.screen = Screen::Params;
        app
    }

    #[test]
    fn destination_explorer_enter_opens_selects_and_closes() {
        let expected = std::env::current_dir().unwrap();
        let mut app = params_app();

        app.on_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        assert!(app.dest_picker.is_some());

        app.on_key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::NONE));

        assert!(matches!(app.screen, Screen::Params));
        assert_eq!(PathBuf::from(app.dest), expected);
        assert!(app.dest_picker.is_none());
    }

    #[test]
    fn destination_explorer_enter_reopens_after_close() {
        let mut app = params_app();

        app.on_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        app.on_key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::NONE));
        assert!(app.dest_picker.is_none());

        app.on_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));

        assert!(app.dest_picker.is_some());
    }

    #[test]
    fn enter_on_dest_opens_explorer_not_worker() {
        let mut app = params_app();

        app.on_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));

        assert!(matches!(app.screen, Screen::Params));
        assert!(app.dest_picker.is_some());
        assert!(app.worker.is_none());
    }

    #[test]
    fn tab_reaches_start_field() {
        let mut app = params_app();

        for _ in 0..3 {
            app.on_key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE));
        }

        assert_eq!(app.field_focus, 3);
    }

    #[test]
    fn destination_explorer_esc_closes_picker_keeps_path() {
        let mut app = params_app();
        let original = app.dest.clone();

        app.on_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        assert!(app.dest_picker.is_some());

        app.on_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));

        assert!(matches!(app.screen, Screen::Params));
        assert!(app.dest_picker.is_none());
        assert_eq!(app.dest, original);
    }

    #[test]
    fn params_esc_with_no_picker_returns_to_menu() {
        let mut app = params_app();

        app.on_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));

        assert!(matches!(app.screen, Screen::Menu));
    }

    #[test]
    fn voice_picker_round_trips_selected_voices() {
        let mut app = params_app();
        app.field_focus = 2;
        app.voice = "en-us,ja-jp".to_owned();

        app.on_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        app.on_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
        app.on_key(KeyEvent::new(KeyCode::Up, KeyModifiers::NONE));
        app.on_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));

        assert!(matches!(app.screen, Screen::Params));
        assert_eq!(app.voice, "en-us,ja-jp");
        assert_eq!(app.field_focus, 3);
    }

    #[test]
    fn voice_picker_can_select_all() {
        let mut app = params_app();
        app.field_focus = 2;

        app.on_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        app.on_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
        app.on_key(KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE));

        assert_eq!(app.voice, "all");
    }

    #[test]
    fn voice_picker_can_select_none() {
        let mut app = params_app();
        app.field_focus = 2;
        app.voice = "en-us,ja-jp".to_owned();

        app.on_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        app.on_key(KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE));

        assert_eq!(app.voice, "none");
    }
}
