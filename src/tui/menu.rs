use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Paragraph, Wrap};
use ratatui::Frame;

use super::app::{App, ACTIONS};
use super::ui::action_icon;

pub(super) fn render_menu(frame: &mut Frame, area: Rect, app: &App) {
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
