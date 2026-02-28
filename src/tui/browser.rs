use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph},
};

use super::app::{App, BrowserItem, Pane, Screen};
use super::dashboard::format_bytes;
use super::preview::render_preview;

pub fn render_browser(f: &mut Frame, app: &App) {
    let state = match &app.screen {
        Screen::Browser(s) => s,
        _ => return,
    };

    // Vertical layout: top panes | preview bar | help bar
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(5),
            Constraint::Length(3),
            Constraint::Length(3),
        ])
        .split(f.area());

    // Split the top area horizontally: left pane | right pane
    let horizontal = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(vertical[0]);

    // --- Left pane title ---
    let left_title = format!(
        " {}: {} ",
        state.user_name, state.current_path
    );
    let left_border_style = if matches!(state.active_pane, Pane::Left) {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default()
    };
    let left_block = Block::default()
        .borders(Borders::ALL)
        .title(left_title)
        .border_style(left_border_style);

    let left_items: Vec<ListItem> = state
        .left_items
        .iter()
        .enumerate()
        .map(|(i, item)| {
            let is_selected = i == state.left_selected;
            let is_marked = state.selected_items.contains(&i);

            let line = match item {
                BrowserItem::ParentDir => {
                    let text = " .. (up)";
                    let style = if is_selected {
                        Style::default().bg(Color::Cyan).fg(Color::Black)
                    } else {
                        Style::default().fg(Color::DarkGray)
                    };
                    Line::from(Span::styled(text, style))
                }
                BrowserItem::Folder(folder) => {
                    let label = format!(" {}/", folder.name);
                    // Count items in the folder from what we know
                    // (We don't have per-subfolder counts in memory, show dash)
                    let right = "".to_string();
                    let base_style = if is_selected {
                        Style::default().bg(Color::Cyan).fg(Color::Black)
                    } else if is_marked {
                        Style::default().fg(Color::Yellow)
                    } else {
                        Style::default().fg(Color::Blue).add_modifier(Modifier::BOLD)
                    };
                    let text = format!("{:<40}{}", label, right);
                    Line::from(Span::styled(text, base_style))
                }
                BrowserItem::File(file) => {
                    let size_str = format_bytes(file.size);
                    let label = format!(" {}", file.name);
                    let text = format!("{:<40}{:>10}", label, size_str);
                    let style = if is_selected {
                        Style::default().bg(Color::Cyan).fg(Color::Black)
                    } else if is_marked {
                        Style::default().fg(Color::Yellow)
                    } else {
                        Style::default()
                    };
                    Line::from(Span::styled(text, style))
                }
            };
            ListItem::new(line)
        })
        .collect();

    let left_list = List::new(left_items).block(left_block);
    f.render_widget(left_list, horizontal[0]);

    // --- Right pane ---
    let right_title = format!(" {} ", state.target_dir.display());
    let right_border_style = if matches!(state.active_pane, Pane::Right) {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default()
    };
    let right_block = Block::default()
        .borders(Borders::ALL)
        .title(right_title)
        .border_style(right_border_style);

    let right_items: Vec<ListItem> = if state.right_entries.is_empty() {
        vec![ListItem::new(Line::from(Span::styled(
            " (empty)",
            Style::default().fg(Color::DarkGray),
        )))]
    } else {
        state
            .right_entries
            .iter()
            .enumerate()
            .map(|(i, name)| {
                let style = if i == state.right_selected
                    && matches!(state.active_pane, Pane::Right)
                {
                    Style::default().bg(Color::Cyan).fg(Color::Black)
                } else if name.ends_with('/') {
                    Style::default().fg(Color::Blue).add_modifier(Modifier::BOLD)
                } else {
                    Style::default()
                };
                ListItem::new(Line::from(Span::styled(format!(" {}", name), style)))
            })
            .collect()
    };

    let right_list = List::new(right_items).block(right_block);
    f.render_widget(right_list, horizontal[1]);

    // --- Preview bar ---
    render_preview(f, vertical[1], app);

    // --- Help bar ---
    let help_line = if let Some(ref msg) = state.status_message {
        Line::from(Span::styled(
            format!(" {}", msg),
            Style::default().fg(Color::Green),
        ))
    } else {
        Line::from(vec![Span::styled(
            " [Enter] Open  [Space] Select  [a] All  [c] Copy  [E] Export  [Tab] Switch  [q] Back",
            Style::default()
                .fg(Color::DarkGray)
                .add_modifier(Modifier::DIM),
        )])
    };
    let help = Paragraph::new(help_line);
    f.render_widget(help, vertical[2]);
}
