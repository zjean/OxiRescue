use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph},
};

use super::app::App;

pub fn format_bytes(bytes: u64) -> String {
    const GB: u64 = 1024 * 1024 * 1024;
    const MB: u64 = 1024 * 1024;
    const KB: u64 = 1024;

    if bytes >= GB {
        format!("{:.2} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.2} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.2} KB", bytes as f64 / KB as f64)
    } else {
        format!("{} B", bytes)
    }
}

pub fn render_dashboard(f: &mut Frame, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Length(8),
            Constraint::Min(5),
            Constraint::Length(3),
        ])
        .split(f.area());

    // 1. Title bar
    let title = Paragraph::new(Line::from(vec![Span::styled(
        " OxiRescue",
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD),
    )]));
    f.render_widget(title, chunks[0]);

    // 2. Stats block
    let stats_block = Block::default()
        .borders(Borders::ALL)
        .title(" Storage Overview ");

    let stats_lines: Vec<Line> = if let Some(stats) = &app.stats {
        let dedup_pct = if stats.logical_bytes > 0 {
            let savings =
                stats.logical_bytes.saturating_sub(stats.physical_bytes) as f64
                    / stats.logical_bytes as f64
                    * 100.0;
            format!("{:.1}%", savings)
        } else {
            "0.0%".to_string()
        };

        vec![
            Line::from(format!(
                "  Users: {}    Files: {}    Folders: {}    Blobs: {}",
                stats.user_count,
                stats.file_count,
                stats.folder_count,
                stats.unique_blobs
            )),
            Line::from(""),
            Line::from(format!(
                "  Logical:  {}",
                format_bytes(stats.logical_bytes)
            )),
            Line::from(format!(
                "  Physical: {}",
                format_bytes(stats.physical_bytes)
            )),
            Line::from(format!("  Dedup savings: {}", dedup_pct)),
        ]
    } else {
        vec![Line::from("  Loading...")]
    };

    let stats_widget = Paragraph::new(stats_lines).block(stats_block);
    f.render_widget(stats_widget, chunks[1]);

    // 3. User list
    let users_block = Block::default()
        .borders(Borders::ALL)
        .title(" Users ");

    let user_items: Vec<ListItem> = app
        .users
        .iter()
        .enumerate()
        .map(|(i, (user, file_count, total_bytes))| {
            let line = format!(
                " {:<20} {:<8} {}",
                user.username,
                file_count,
                format_bytes(*total_bytes)
            );
            let style = if i == app.selected_user {
                Style::default()
                    .bg(Color::Cyan)
                    .fg(Color::Black)
            } else {
                Style::default()
            };
            ListItem::new(Line::from(Span::styled(line, style)))
        })
        .collect();

    let users_list = List::new(user_items).block(users_block);
    f.render_widget(users_list, chunks[2]);

    // 4. Help bar
    let help_text = Paragraph::new(Line::from(vec![Span::styled(
        " q: Quit   j/↓: Down   k/↑: Up   Enter: Browse user files",
        Style::default()
            .fg(Color::DarkGray)
            .add_modifier(Modifier::DIM),
    )]));
    f.render_widget(help_text, chunks[3]);
}
