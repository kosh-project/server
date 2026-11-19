mod core;
mod ui;

use std::time::Duration;

use anyhow::anyhow;
use crossterm::{
    event::{
        DisableBracketedPaste, DisableFocusChange, EnableBracketedPaste,
        EnableFocusChange, EventStream, KeyboardEnhancementFlags,
        PopKeyboardEnhancementFlags, PushKeyboardEnhancementFlags,
    },
    execute,
};
use futures::StreamExt;
use kosh_core::config::Config;
use ratatui::{DefaultTerminal, widgets::Widget};
#[cfg(unix)]
use tokio::fs;
#[cfg(not(unix))]
use tokio::net::UdpSocket;
#[cfg(unix)]
use tokio::net::UnixDatagram;
use tokio::time::interval;

use crate::ui::app::App;

#[cfg(unix)]
async fn bind_telemetry_socket(
    socket_path: &str,
) -> anyhow::Result<UnixDatagram> {
    let _ = fs::remove_file(socket_path).await;
    let socket = UnixDatagram::bind(socket_path)?;
    Ok(socket)
}

#[cfg(not(unix))]
async fn bind_telemetry_socket(socket_path: &str) -> anyhow::Result<UdpSocket> {
    let addr =
        socket_path
            .parse::<std::net::SocketAddr>()
            .unwrap_or_else(|_| {
                std::net::SocketAddr::from(([127, 0, 0, 1], 23343))
            });
    let socket = UdpSocket::bind(addr).await?;
    Ok(socket)
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let config = Config::load_or_init().await?;
    color_eyre::install().map_err(|e| anyhow!("{e}"))?;

    let mut term = ratatui::init();

    execute!(
        std::io::stdout(),
        // EnableMouseCapture,
        EnableBracketedPaste,
        PushKeyboardEnhancementFlags(
            KeyboardEnhancementFlags::REPORT_EVENT_TYPES
        ),
        EnableFocusChange,
    )?;

    #[allow(clippy::large_futures)]
    let result = app(&mut term, config).await;

    execute!(
        std::io::stdout(),
        // DisableMouseCapture,
        DisableBracketedPaste,
        PopKeyboardEnhancementFlags,
        DisableFocusChange
    )?;

    ratatui::restore();
    result?;

    Ok(())
}

async fn app(
    terminal: &mut DefaultTerminal,
    config: Config,
) -> anyhow::Result<()> {
    let mut app: App = App::try_init(&config)
        .await
        .ok_or_else(|| anyhow!("Failed to initiate app"))?;

    let mut event_stream = EventStream::new();
    let socket = bind_telemetry_socket(&config.socket_path).await?;

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
                if app.last_seen.elapsed().as_secs() >= 5 {
                    app.pair_info.is_online = false;
                }
                needs_render = true;
            }
        }
    }

    Ok(())
}
