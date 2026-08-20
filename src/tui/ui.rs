use ratatui::layout::{Alignment, Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Paragraph, Tabs};
use ratatui::Frame;

use super::app::{Action, App, Screen};
use super::menu::render_menu;
use super::params::render_params;
use super::progress::render_progress;
use super::result::render_result;
use crate::edition::Edition;

pub(super) fn ui(frame: &mut Frame, app: &mut App) {
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

fn render_footer(frame: &mut Frame, area: Rect, app: &App) {
    let line: Line = match app.screen {
        Screen::Menu => hint_line(&[
            ("←/→", "edition"),
            ("↑/↓", "select"),
            ("Enter", "confirm"),
            ("q", "quit"),
        ]),
        Screen::Params => {
            if app.dest_picker.is_some() {
                hint_line(&[
                    ("Esc", "close"),
                    ("↑/↓", "move"),
                    ("Enter", "open"),
                    ("Backspace", "parent"),
                    ("c", "choose current destination"),
                ])
            } else if app.voice_picker.is_some() {
                hint_line(&[
                    ("↑/↓", "move"),
                    ("Space", "toggle"),
                    ("Enter", "next"),
                    ("Esc", "back"),
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
                " quit (stops the download)",
                Style::default().fg(Color::DarkGray),
            ));
            line
        }
        Screen::Result => hint_line(&[("↑/↓", "scroll"), ("Enter/Esc", "back to menu")]),
    };

    let footer = Paragraph::new(line).block(
        Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(Color::White)),
    );
    frame.render_widget(footer, area);
}

pub(super) fn hint_line<'a>(hints: &[(&'a str, &'a str)]) -> Line<'a> {
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

pub(super) fn action_icon(action: Action) -> &'static str {
    match action {
        Action::Download => "⬇",
        Action::Update => "🔄",
        Action::Predownload => "📥",
        Action::Repair => "🛠",
        Action::List => "📋",
        Action::Quit => "🚪",
    }
}
