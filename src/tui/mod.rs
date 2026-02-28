pub mod app;
mod dashboard;

use std::io::stdout;
use std::time::Duration;

use crossterm::{
    event::{self, Event, KeyCode},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{Terminal, backend::CrosstermBackend, widgets::Paragraph};

use app::{App, Screen};

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
                Screen::Browser { .. } => {
                    // Placeholder — Task 9 will implement this
                    let msg = Paragraph::new(
                        "Browser — not yet implemented. Press q to go back.",
                    );
                    f.render_widget(msg, f.area());
                }
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
                                app.screen = Screen::Browser {
                                    user_id: user.id.clone(),
                                    user_name: user.username.clone(),
                                };
                            }
                        }
                        _ => {}
                    },
                    Screen::Browser { .. } => match key.code {
                        KeyCode::Char('q') | KeyCode::Esc => {
                            app.screen = Screen::Dashboard;
                        }
                        _ => {}
                    },
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
