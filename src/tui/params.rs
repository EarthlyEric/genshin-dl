use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Paragraph, Wrap};
use ratatui::Frame;
use tui_file_explorer::render as render_file_explorer;

use super::app::{App, ACTIONS};
use super::ui::action_icon;
use super::voice_picker::render_voice_picker;

pub(super) fn render_params(frame: &mut Frame, area: Rect, app: &mut App) {
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
