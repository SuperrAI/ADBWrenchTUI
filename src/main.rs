mod adb;
mod app;
mod components;
mod config;
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

    // Detect terminal image protocol BEFORE entering alternate screen
    let picker = ratatui_image::picker::Picker::from_query_stdio()
        .unwrap_or_else(|_| ratatui_image::picker::Picker::halfblocks());
    tracing::info!("Image protocol: {:?}", picker.protocol_type());

    // Initialize terminal
    let mut terminal = tui::init()?;

    // Create app state
    let mut app = App::new(Some(picker));

    // Try to detect connected devices
    if let Err(e) = app.init_device().await {
        tracing::warn!("Failed to detect devices: {e}");
    }

    // Event handler (tick every 100ms)
    let mut events = EventHandler::new(Duration::from_millis(100));

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
            }
            Event::Mouse(mouse) => {
                let action = app.handle_mouse(mouse);
                if !matches!(action, AppAction::None) {
                    app.dispatch_action(action).await;
                }
            }
            Event::Resize(_w, _h) => {
                // Terminal auto-handles resize
            }
            Event::Tick => {}
        }

        app.process_background().await;
    }

    // Restore terminal
    tui::restore(&mut terminal)?;

    tracing::info!("ADBWrenchTUI exiting");
    Ok(())
}
