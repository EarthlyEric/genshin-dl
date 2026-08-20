use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::mpsc;
use std::time::Duration;

use anime_launcher_sdk::anime_game_core::sophon::prettify_bytes;
use crossterm::event::{self, Event as TermEvent, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::DefaultTerminal;
use tui_file_explorer::{ExplorerOutcome, FileExplorer};

use super::ui::ui;
use super::voice_picker::{VoicePicker, VOICE_OPTIONS};
use crate::cmd;
use crate::download::{self, Event};
use crate::edition::Edition;
use crate::voice;

const LOG_LIMIT: usize = 100;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Action {
    Download,
    Update,
    Predownload,
    Repair,
    List,
    Quit,
}

pub(super) const ACTIONS: [(Action, &str); 6] = [
    (Action::Download, "Download game"),
    (Action::Update, "Update game"),
    (Action::Predownload, "Pre-download update"),
    (Action::Repair, "Repair / check files"),
    (Action::List, "List versions"),
    (Action::Quit, "Quit"),
];

#[derive(PartialEq, Eq)]
pub(super) enum Screen {
    Menu,
    Params,
    Progress,
    Result,
}

#[derive(Default)]
pub(super) struct Progress {
    pub(super) done: u64,
    pub(super) total: u64,
}

impl Progress {
    pub(super) fn set(&mut self, done: u64, total: u64) {
        self.done = done;
        self.total = total;
    }

    pub(super) fn ratio(&self) -> f64 {
        if self.total == 0 {
            0.0
        } else {
            (self.done as f64 / self.total as f64).clamp(0.0, 1.0)
        }
    }

    pub(super) fn label(&self) -> String {
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

pub(super) struct App {
    pub(super) edition: Edition,
    pub(super) screen: Screen,
    pub(super) menu_idx: usize,
    pub(super) action: Action,
    pub(super) dest: String,
    pub(super) threads: String,
    pub(super) voice: String,
    pub(super) field_focus: usize,
    pub(super) phase: String,
    pub(super) bytes: Progress,
    pub(super) files: Progress,
    pub(super) logs: VecDeque<String>,
    tracing_rx: mpsc::Receiver<String>,
    pub(super) dest_picker: Option<FileExplorer>,
    pub(super) voice_picker: Option<VoicePicker>,
    rx: Option<mpsc::Receiver<Event>>,
    worker: Option<std::thread::JoinHandle<anyhow::Result<()>>>,
    pub(super) result: Option<Result<(), String>>,
    pub(super) list_output: Vec<String>,
    pub(super) result_scroll: usize,
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
            result_scroll: 0,
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
                KeyCode::Esc | KeyCode::Enter => {
                    self.result_scroll = 0;
                    self.screen = Screen::Menu;
                }
                KeyCode::Down => self.result_scroll += 1,
                KeyCode::Up => self.result_scroll = self.result_scroll.saturating_sub(1),
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
            KeyCode::Char(c) if self.field_focus == 1 && c.is_ascii_digit() => {
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
        self.result_scroll = 0;

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
        let threads = parse_threads(&self.threads);
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
                self.result_scroll = 0;
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

fn parse_threads(input: &str) -> usize {
    input.trim().parse().ok().filter(|t| *t > 0).unwrap_or(8)
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

    #[test]
    fn threads_field_rejects_non_digits() {
        let mut app = params_app();
        app.field_focus = 1;

        app.on_key(KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE));
        assert_eq!(app.threads, "8");

        app.on_key(KeyEvent::new(KeyCode::Char('1'), KeyModifiers::NONE));
        assert_eq!(app.threads, "81");
    }

    #[test]
    fn parse_threads_falls_back_and_rejects_zero() {
        assert_eq!(parse_threads("12"), 12);
        assert_eq!(parse_threads("0"), 8);
        assert_eq!(parse_threads("abc"), 8);
        assert_eq!(parse_threads(""), 8);
    }

    #[test]
    fn result_screen_scrolls_and_resets_on_back() {
        let (_, tracing_rx) = mpsc::channel();
        let mut app = App::new(Edition::Global, tracing_rx);
        app.screen = Screen::Result;
        app.result_scroll = 0;

        app.on_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
        assert_eq!(app.result_scroll, 1);
        app.on_key(KeyEvent::new(KeyCode::Up, KeyModifiers::NONE));
        assert_eq!(app.result_scroll, 0);
        app.on_key(KeyEvent::new(KeyCode::Up, KeyModifiers::NONE));
        assert_eq!(app.result_scroll, 0);

        app.on_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
        app.on_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
        assert!(matches!(app.screen, Screen::Menu));
        assert_eq!(app.result_scroll, 0);
    }


}
