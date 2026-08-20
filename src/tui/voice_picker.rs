use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, List, ListItem, Padding, Paragraph};
use ratatui::Frame;

use crate::voice;

pub(super) const VOICE_OPTIONS: [(&str, &str); 6] = [
    ("none", "None"),
    ("all", "All voices"),
    ("en-us", "English"),
    ("ja-jp", "Japanese"),
    ("ko-kr", "Korean"),
    ("zh-cn", "Chinese"),
];

pub(super) struct VoicePicker {
    selected: [bool; VOICE_OPTIONS.len()],
    pub(super) cursor: usize,
}

impl VoicePicker {
    pub(super) fn from_value(value: &str) -> Self {
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

    pub(super) fn toggle_current(&mut self) {
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

    pub(super) fn value(&self) -> String {
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

pub(super) fn render_voice_picker(frame: &mut Frame, area: Rect, picker: &VoicePicker) {
    let chunks = Layout::vertical([Constraint::Length(3), Constraint::Min(1)]).split(area);

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
}
