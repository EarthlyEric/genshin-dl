use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Paragraph, Wrap};
use ratatui::Frame;

use super::app::App;

pub(super) fn render_result(frame: &mut Frame, area: Rect, app: &App) {
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
