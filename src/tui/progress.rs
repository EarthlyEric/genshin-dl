use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Gauge, Paragraph, Wrap};
use ratatui::Frame;

use super::app::App;

const MIN_GAUGE_WIDTH: u16 = 36;

pub(super) fn render_progress(frame: &mut Frame, area: Rect, app: &App) {
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
