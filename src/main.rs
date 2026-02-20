mod adb;
mod app;
mod components;
mod event;
mod theme;
mod tui;
mod ui;

use std::time::Duration;

use anyhow::Result;

use app::App;
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
                // Let the app handle global keys first
                if !app.handle_key(key) {
                    // TODO: forward to active page handler
                }
            }
            Event::Mouse(_mouse) => {
                // TODO: mouse handling
            }
            Event::Resize(_w, _h) => {
                // Terminal auto-handles resize
            }
            Event::Tick => {
                // TODO: periodic data refresh
            }
        }
    }

    // Restore terminal
    tui::restore(&mut terminal)?;

    tracing::info!("ADBWrenchTUI exiting");
    Ok(())
}
