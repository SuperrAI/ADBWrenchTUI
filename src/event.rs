use std::time::Duration;

use anyhow::Result;
use crossterm::event::{self, Event as CrosstermEvent, KeyEvent, MouseEvent};
use tokio::sync::mpsc;

/// Application events.
#[derive(Debug, Clone)]
pub enum Event {
    /// Terminal key press
    Key(KeyEvent),
    /// Terminal mouse event
    Mouse(MouseEvent),
    /// Terminal resize
    Resize(u16, u16),
    /// Periodic tick for UI updates (e.g., performance data refresh)
    Tick,
}

/// Event handler that polls crossterm events and forwards them over an mpsc channel.
pub struct EventHandler {
    rx: mpsc::UnboundedReceiver<Event>,
    _tx: mpsc::UnboundedSender<Event>,
}

impl EventHandler {
    /// Create a new event handler with the given tick rate.
    pub fn new(tick_rate: Duration) -> Self {
        let (tx, rx) = mpsc::unbounded_channel();
        let _tx = tx.clone();

        tokio::spawn(async move {
            loop {
                let event_available =
                    event::poll(tick_rate).unwrap_or(false);

                if event_available {
                    match event::read() {
                        Ok(CrosstermEvent::Key(key)) => {
                            if tx.send(Event::Key(key)).is_err() {
                                return;
                            }
                        }
                        Ok(CrosstermEvent::Mouse(mouse)) => {
                            if tx.send(Event::Mouse(mouse)).is_err() {
                                return;
                            }
                        }
                        Ok(CrosstermEvent::Resize(w, h)) => {
                            if tx.send(Event::Resize(w, h)).is_err() {
                                return;
                            }
                        }
                        _ => {}
                    }
                } else {
                    // No event within tick_rate → emit tick
                    if tx.send(Event::Tick).is_err() {
                        return;
                    }
                }
            }
        });

        Self { rx, _tx }
    }

    /// Wait for the next event.
    pub async fn next(&mut self) -> Result<Event> {
        self.rx
            .recv()
            .await
            .ok_or_else(|| anyhow::anyhow!("Event channel closed"))
    }
}
