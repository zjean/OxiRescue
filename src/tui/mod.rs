pub mod app;
mod browser;
mod dashboard;
mod export;
mod preview;

use std::io::stdout;
use std::time::Duration;

use crossterm::{
    event::{self, Event, KeyCode},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{Terminal, backend::CrosstermBackend};

use app::{App, BrowserItem, Pane, Screen};

pub fn run_tui(mut app: App) -> anyhow::Result<()> {
    app.load_dashboard()?;

    enable_raw_mode()?;
    let mut stdout = stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let mut terminal = Terminal::new(CrosstermBackend::new(stdout))?;

    loop {
        terminal.draw(|f| {
            match &app.screen {
                Screen::Dashboard => dashboard::render_dashboard(f, &app),
                Screen::Browser(_) => browser::render_browser(f, &app),
            }
        })?;

        if event::poll(Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                match &app.screen {
                    Screen::Dashboard => match key.code {
                        KeyCode::Char('q') => {
                            app.should_quit = true;
                        }
                        KeyCode::Up | KeyCode::Char('k') => {
                            if app.selected_user > 0 {
                                app.selected_user -= 1;
                            }
                        }
                        KeyCode::Down | KeyCode::Char('j') => {
                            if app.selected_user + 1 < app.users.len() {
                                app.selected_user += 1;
                            }
                        }
                        KeyCode::Enter => {
                            if let Some((user, _, _)) = app.users.get(app.selected_user) {
                                let user_id = user.id.clone();
                                let user_name = user.username.clone();
                                let _ = app.enter_browser(user_id, user_name);
                            }
                        }
                        _ => {}
                    },
                    Screen::Browser(_) => {
                        handle_browser_key(&mut app, key.code)?;
                    }
                }
            }
        }

        if app.should_quit {
            break;
        }
    }

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    Ok(())
}

fn handle_browser_key(app: &mut App, code: KeyCode) -> anyhow::Result<()> {
    match code {
        KeyCode::Char('q') | KeyCode::Esc => {
            app.screen = Screen::Dashboard;
        }

        KeyCode::Tab => {
            if let Screen::Browser(ref mut state) = app.screen {
                state.active_pane = match state.active_pane {
                    Pane::Left => Pane::Right,
                    Pane::Right => Pane::Left,
                };
                // Clear status message on tab switch
                state.status_message = None;
            }
        }

        KeyCode::Down | KeyCode::Char('j') => {
            if let Screen::Browser(ref mut state) = app.screen {
                match state.active_pane {
                    Pane::Left => {
                        if !state.left_items.is_empty()
                            && state.left_selected + 1 < state.left_items.len()
                        {
                            state.left_selected += 1;
                        }
                    }
                    Pane::Right => {
                        if !state.right_entries.is_empty()
                            && state.right_selected + 1 < state.right_entries.len()
                        {
                            state.right_selected += 1;
                        }
                    }
                }
            }
        }

        KeyCode::Up | KeyCode::Char('k') => {
            if let Screen::Browser(ref mut state) = app.screen {
                match state.active_pane {
                    Pane::Left => {
                        if state.left_selected > 0 {
                            state.left_selected -= 1;
                        }
                    }
                    Pane::Right => {
                        if state.right_selected > 0 {
                            state.right_selected -= 1;
                        }
                    }
                }
            }
        }

        KeyCode::Enter => {
            // Navigate into folder or ParentDir
            let (folder_id_to_load, navigate_up, current_folder_id) = {
                if let Screen::Browser(ref state) = app.screen {
                    if !matches!(state.active_pane, Pane::Left) {
                        (None, false, None)
                    } else {
                        match state.left_items.get(state.left_selected) {
                            Some(BrowserItem::ParentDir) => {
                                // Navigate up: we need to find parent of current folder
                                (None, true, state.current_folder_id.clone())
                            }
                            Some(BrowserItem::Folder(folder)) => {
                                (Some(folder.id.clone()), false, None)
                            }
                            _ => (None, false, None),
                        }
                    }
                } else {
                    (None, false, None)
                }
            };

            if let Some(fid) = folder_id_to_load {
                // Navigate into folder
                // Get folder name and path first
                let (folder_name, folder_path) = {
                    if let Screen::Browser(ref state) = app.screen {
                        let item = state.left_items.get(state.left_selected);
                        match item {
                            Some(BrowserItem::Folder(f)) => (f.name.clone(), f.path.clone()),
                            _ => (String::new(), String::new()),
                        }
                    } else {
                        (String::new(), String::new())
                    }
                };
                if let Screen::Browser(ref mut state) = app.screen {
                    state.current_folder_id = Some(fid.clone());
                    state.current_path = if folder_path.is_empty() {
                        format!("/{}", folder_name)
                    } else {
                        folder_path
                    };
                }
                app.load_folder(Some(&fid))?;
            } else if navigate_up {
                // Navigate up: find parent folder of current_folder_id
                let parent_folder_id = find_parent_folder(app, current_folder_id.as_deref());
                if let Screen::Browser(ref mut state) = app.screen {
                    state.current_folder_id = parent_folder_id.clone();
                    state.current_path = if parent_folder_id.is_none() {
                        "/".to_string()
                    } else {
                        // We don't have easy path lookup, use generic
                        "..".to_string()
                    };
                }
                let fid_opt = parent_folder_id.clone();
                app.load_folder(fid_opt.as_deref())?;
                // After loading, if we're at root, rebuild path
                if parent_folder_id.is_none() {
                    if let Screen::Browser(ref mut state) = app.screen {
                        state.current_path = "/".to_string();
                    }
                }
            }
        }

        KeyCode::Char(' ') => {
            if let Screen::Browser(ref mut state) = app.screen {
                if matches!(state.active_pane, Pane::Left) {
                    let idx = state.left_selected;
                    if !state.left_items.is_empty() {
                        if state.selected_items.contains(&idx) {
                            state.selected_items.remove(&idx);
                        } else {
                            state.selected_items.insert(idx);
                        }
                    }
                }
            }
        }

        KeyCode::Char('a') => {
            if let Screen::Browser(ref mut state) = app.screen {
                if matches!(state.active_pane, Pane::Left) {
                    // Select all files (not ParentDir, not folders)
                    let file_indices: Vec<usize> = state
                        .left_items
                        .iter()
                        .enumerate()
                        .filter_map(|(i, item)| {
                            if matches!(item, BrowserItem::File(_)) {
                                Some(i)
                            } else {
                                None
                            }
                        })
                        .collect();
                    for idx in file_indices {
                        state.selected_items.insert(idx);
                    }
                }
            }
        }

        KeyCode::Char('c') => {
            // Copy all selected files to target_dir
            let (files_to_copy, target_dir) = {
                if let Screen::Browser(ref state) = app.screen {
                    let mut files = Vec::new();
                    for &idx in &state.selected_items {
                        if let Some(BrowserItem::File(f)) = state.left_items.get(idx) {
                            files.push(f.clone());
                        }
                    }
                    (files, state.target_dir.clone())
                } else {
                    (Vec::new(), std::path::PathBuf::new())
                }
            };

            let mut copied = 0usize;
            let mut errors = 0usize;
            for file in &files_to_copy {
                match export::export_file(&app.blobs, file, &target_dir) {
                    Ok(()) => copied += 1,
                    Err(_) => errors += 1,
                }
            }

            app.refresh_right_pane();

            if let Screen::Browser(ref mut state) = app.screen {
                state.selected_items.clear();
                state.status_message = if errors > 0 {
                    Some(format!(
                        "Copied {} files ({} errors)",
                        copied, errors
                    ))
                } else {
                    Some(format!("Copied {} files", copied))
                };
            }
        }

        KeyCode::Char('E') => {
            // Export current folder subtree to target_dir
            let (folder_to_export, target_dir) = {
                if let Screen::Browser(ref state) = app.screen {
                    // Find currently highlighted folder, or current folder
                    let folder = match state.left_items.get(state.left_selected) {
                        Some(BrowserItem::Folder(f)) => Some(f.clone()),
                        _ => None,
                    };
                    (folder, state.target_dir.clone())
                } else {
                    (None, std::path::PathBuf::new())
                }
            };

            if let Some(folder) = folder_to_export {
                let meta = &*app.meta;
                let result = export::export_folder(&app.blobs, meta, &folder, &target_dir);
                app.refresh_right_pane();
                if let Screen::Browser(ref mut state) = app.screen {
                    state.status_message = match result {
                        Ok(n) => Some(format!("Exported {} files from '{}'", n, folder.name)),
                        Err(e) => Some(format!("Export error: {}", e)),
                    };
                }
            } else {
                if let Screen::Browser(ref mut state) = app.screen {
                    state.status_message = Some("Select a folder first".to_string());
                }
            }
        }

        _ => {}
    }
    Ok(())
}

/// Find the parent folder ID of `current_folder_id` by looking in app.meta.
/// Returns None if current is already root or parent is root.
fn find_parent_folder(app: &App, current_folder_id: Option<&str>) -> Option<String> {
    let fid = current_folder_id?;
    // List all folders for the user and find the one with id == fid,
    // then return its parent_id.
    if let Screen::Browser(ref state) = app.screen {
        let user_id = &state.user_id;
        if let Ok(all_folders) = app.meta.list_folders_for_user(user_id) {
            for folder in all_folders {
                if folder.id == fid {
                    return folder.parent_id;
                }
            }
        }
    }
    None
}
