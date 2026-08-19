use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::mpsc;
use std::time::Duration;

use anime_launcher_sdk::anime_game_core::sophon::prettify_bytes;
use crossterm::event::{self, Event as TermEvent, KeyCode, KeyEvent, KeyEventKind};
use ratatui::layout::{Alignment, Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Gauge, Paragraph, Tabs, Wrap};
use ratatui::{DefaultTerminal, Frame};

use crate::cmd;
use crate::download::{self, Event};
use crate::edition::Edition;
use crate::voice;

const LOG_LIMIT: usize = 100;

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

pub fn run(edition: Edition) -> anyhow::Result<()> {
    let mut terminal = ratatui::init();
    let result = run_app(&mut terminal, edition);
    ratatui::restore();
    result
}

fn run_app(terminal: &mut DefaultTerminal, edition: Edition) -> anyhow::Result<()> {
    let mut app = App::new(edition);

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
    rx: Option<mpsc::Receiver<Event>>,
    worker: Option<std::thread::JoinHandle<anyhow::Result<()>>>,
    result: Option<Result<(), String>>,
    list_output: Vec<String>,
    quit: bool,
}

impl App {
    fn new(edition: Edition) -> Self {
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
        match key.code {
            KeyCode::Esc => self.screen = Screen::Menu,
            KeyCode::Enter => self.start_worker(),
            KeyCode::Down | KeyCode::Tab => {
                self.field_focus = (self.field_focus + 1) % 3;
            }
            KeyCode::Up => {
                self.field_focus = (self.field_focus + 2) % 3;
            }
            KeyCode::Backspace => {
                let field = match self.field_focus {
                    0 => &mut self.dest,
                    1 => &mut self.threads,
                    _ => &mut self.voice,
                };
                field.pop();
            }
            KeyCode::Char(c) => {
                let field = match self.field_focus {
                    0 => &mut self.dest,
                    1 => &mut self.threads,
                    _ => &mut self.voice,
                };
                field.push(c);
            }
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
        let events: Vec<Event> = match &self.rx {
            Some(rx) => rx.try_iter().collect(),
            None => return,
        };

        for event in events {
            self.handle_event(event);
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
        Constraint::Length(1),
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
        .map(|(idx, (_, label))| {
            let selected = idx == app.menu_idx;
            Line::from(Span::styled(
                if selected {
                    format!("> {label}")
                } else {
                    format!("  {label}")
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
        .block(Block::bordered().title(" Select an action "))
        .wrap(Wrap { trim: true });

    frame.render_widget(paragraph, area);
}

fn render_params(frame: &mut Frame, area: Rect, app: &mut App) {
    let fields = [
        ("Game directory", app.dest.clone()),
        ("Threads", app.threads.clone()),
        ("Voices (comma separated)", app.voice.clone()),
    ];
    let mut lines = Vec::new();

    for (idx, (label, value)) in fields.iter().enumerate() {
        let focused = idx == app.field_focus;
        let marker = if focused { ">" } else { " " };
        lines.push(Line::from(format!("{marker} {label}:")));
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

    lines.push(Line::from(format!(
        "\nAction: {}",
        ACTIONS
            .iter()
            .find(|(a, _)| *a == app.action)
            .map(|(_, l)| *l)
            .unwrap_or("")
    )));

    let paragraph = Paragraph::new(lines)
        .block(Block::bordered().title(" Parameters "))
        .wrap(Wrap { trim: true });

    frame.render_widget(paragraph, area);
}

fn render_progress(frame: &mut Frame, area: Rect, app: &mut App) {
    let chunks = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(3),
        Constraint::Length(1),
        Constraint::Length(3),
        Constraint::Min(0),
    ])
    .split(area);

    let phase = Paragraph::new(Span::styled(
        app.phase.clone(),
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD),
    ));
    frame.render_widget(phase, chunks[0]);

    let bytes_label = format!("{} ({:.1}%)", app.bytes.label(), app.bytes.ratio() * 100.0);
    let bytes_gauge = Gauge::default()
        .block(Block::bordered().title(" Bytes "))
        .ratio(app.bytes.ratio())
        .label(bytes_label)
        .gauge_style(Style::default().fg(Color::Cyan));
    frame.render_widget(bytes_gauge, chunks[2]);

    let files_label = format!("{} / {} files", app.files.done, app.files.total);
    let files_gauge = Gauge::default()
        .block(Block::bordered().title(" Files "))
        .ratio(app.files.ratio())
        .label(files_label)
        .gauge_style(Style::default().fg(Color::Green));
    frame.render_widget(files_gauge, chunks[4]);

    let log_text: Vec<Line> = app
        .logs
        .iter()
        .map(|l| Line::from(Span::styled(l.clone(), Style::default().fg(Color::Gray))))
        .collect();

    let log = Paragraph::new(log_text)
        .block(Block::bordered().title(" Log "))
        .wrap(Wrap { trim: true });

    frame.render_widget(log, chunks[5]);
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
    let hint = match app.screen {
        Screen::Menu => "←/→ edition   ↑/↓ select   Enter confirm   q quit",
        Screen::Params => "Tab/↑↓ field   Enter start   Esc back",
        Screen::Progress => "Working... q quit (worker keeps running)",
        Screen::Result => "Enter/Esc back to menu",
    };

    let footer = Paragraph::new(Span::styled(hint, Style::default().fg(Color::DarkGray)));
    frame.render_widget(footer, area);
}
