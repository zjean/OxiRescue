use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
};

use super::app::{App, BrowserItem, Screen};
use super::dashboard::format_bytes;

pub fn render_preview(f: &mut Frame, area: Rect, app: &App) {
    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Preview ");

    let content = if let Screen::Browser(ref state) = app.screen {
        if let Some(item) = state.left_items.get(state.left_selected) {
            match item {
                BrowserItem::ParentDir => {
                    Line::from(Span::styled(
                        " Go up one directory",
                        Style::default().fg(Color::DarkGray),
                    ))
                }
                BrowserItem::Folder(folder) => {
                    // Count items: subfolders + files shown in current list that belong to it
                    // We use the in-memory folders/files for immediate context
                    let item_count = state
                        .left_items
                        .iter()
                        .filter(|i| matches!(i, BrowserItem::Folder(_) | BrowserItem::File(_)))
                        .count();
                    Line::from(vec![
                        Span::styled(
                            format!(" {}", folder.name),
                            Style::default().fg(Color::Cyan),
                        ),
                        Span::raw(format!(" | {} items | path: {}", item_count, folder.path)),
                    ])
                }
                BrowserItem::File(file) => {
                    let size_str = format_bytes(file.size);
                    let hash_prefix = if file.blob_hash.len() >= 8 {
                        format!("{}...", &file.blob_hash[..8])
                    } else {
                        file.blob_hash.clone()
                    };
                    let created = file
                        .created_at
                        .as_deref()
                        .unwrap_or("unknown");
                    Line::from(vec![
                        Span::styled(
                            format!(" {}", file.name),
                            Style::default().fg(Color::Yellow),
                        ),
                        Span::raw(format!(
                            " | {} | blob: {} | {} | {}",
                            size_str, hash_prefix, file.mime_type, created
                        )),
                    ])
                }
            }
        } else {
            Line::from(Span::styled(
                " No item selected",
                Style::default().fg(Color::DarkGray),
            ))
        }
    } else {
        Line::from("")
    };

    let widget = Paragraph::new(content).block(block);
    f.render_widget(widget, area);
}
