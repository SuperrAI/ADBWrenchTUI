mod adb;
mod app;
mod components;
mod event;
mod theme;
mod tui;
mod ui;

use std::time::Duration;

use anyhow::Result;

use app::{App, AppAction};
use event::{Event, EventHandler};

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging to file (not visible in TUI)
    let log_file = tracing_appender::rolling::never(".", "adbwrenchtui.log");
    tracing_subscriber::fmt()
        .with_env_filter("adbwrenchtui=debug")
        .with_writer(log_file)
        .with_ansi(false)
        .init();

    tracing::info!("ADBWrenchTUI starting");

    // Initialize terminal
    let mut terminal = tui::init()?;

    // Create app state
    let mut app = App::new();

    // Try to detect connected devices
    if let Err(e) = app.init_device().await {
        tracing::warn!("Failed to detect devices: {e}");
    }

    // Event handler (tick every 250ms)
    let mut events = EventHandler::new(Duration::from_millis(250));

    // Main loop
    while app.running {
        // Render
        terminal.draw(|frame| {
            ui::render(&app, frame);
        })?;

        // Handle events
        match events.next().await? {
            Event::Key(key) => {
                // Let the app handle global keys first, then page-specific
                if !app.handle_key(key) {
                    let action = app.handle_page_key(key);
                    if !matches!(action, AppAction::None) {
                        app.dispatch_action(action).await;
                    }
                }
                // Check for pending action from modal confirmation
                if let Some(action) = app.pending_action.take() {
                    app.dispatch_action(action).await;
                }
            }
            Event::Mouse(mouse) => {
                app.handle_mouse(mouse);
            }
            Event::Resize(_w, _h) => {
                // Terminal auto-handles resize
            }
            Event::Tick => {
                // Dashboard auto-refresh
                if app.dashboard_needs_refresh() {
                    app.refresh_dashboard().await;
                }

                // Performance polling
                if app.perf_needs_collect() {
                    app.collect_perf_data().await;
                }

                // Drain streaming channels
                app.drain_shell_output();
                app.drain_logcat_lines();
                app.drain_bugreport_progress();

                // Update recording elapsed
                app.update_screen_recording();

                // Clear stale result messages
                app.clear_stale_results();
            }
        }
    }

    // Restore terminal
    tui::restore(&mut terminal)?;

    tracing::info!("ADBWrenchTUI exiting");
    Ok(())
}
