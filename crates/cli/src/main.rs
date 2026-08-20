mod app;
mod entry;
mod help;

use std::time::Duration;

use anyhow::anyhow;
use crossterm::{
    event::{
        DisableBracketedPaste, DisableFocusChange, DisableMouseCapture,
        EnableBracketedPaste, EnableFocusChange, EnableMouseCapture,
        EventStream, KeyboardEnhancementFlags, PopKeyboardEnhancementFlags,
        PushKeyboardEnhancementFlags,
    },
    execute,
};
use futures::StreamExt;
use ratatui::{DefaultTerminal, widgets::Widget};
use tokio::{fs, net::UnixDatagram, time::interval};
use webdav_server::SOCKET_ADDR;

use crate::app::App;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    color_eyre::install().map_err(|e| anyhow!("{e}"))?;

    let mut term = ratatui::init();

    execute!(
        std::io::stdout(),
        EnableMouseCapture,
        EnableBracketedPaste,
        PushKeyboardEnhancementFlags(
            KeyboardEnhancementFlags::REPORT_EVENT_TYPES
        ),
        EnableFocusChange,
    )?;

    #[allow(clippy::large_futures)]
    let result = app(&mut term).await;

    execute!(
        std::io::stdout(),
        DisableMouseCapture,
        DisableBracketedPaste,
        PopKeyboardEnhancementFlags,
        DisableFocusChange
    )?;

    ratatui::restore();
    result?;

    Ok(())
}

async fn app(terminal: &mut DefaultTerminal) -> anyhow::Result<()> {
    let mut app: App =
        App::try_init().ok_or_else(|| anyhow!("Failed to initiate app"))?;

    let mut event_stream = EventStream::new();
    let _ = fs::remove_file(SOCKET_ADDR).await;
    let socket = UnixDatagram::bind(SOCKET_ADDR)?;

    #[allow(clippy::large_stack_arrays)]
    let mut buffer = [0u8; 65536];

    let mut ticker = interval(Duration::from_millis(100));
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let mut needs_render = true;

    while !app.should_quit {
        if needs_render {
            terminal.draw(|f| app.render(f.area(), f.buffer_mut()))?;
            needs_render = false;
        }

        tokio::select! {
            Some(event) = event_stream.next() => {
                app.handle_event(&event?);
                needs_render = true;
            },
            Ok(len) = socket.recv(&mut buffer) => {
                app.append(&buffer[..len]);
            },
            _ = ticker.tick() => {
                needs_render = true;
            }

        }
    }

    Ok(())
}
