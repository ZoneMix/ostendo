use std::sync::mpsc;
use std::thread;

use futures_util::{SinkExt, StreamExt};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::sync::broadcast;
use tokio_tungstenite::tungstenite::Message;

use super::html::REMOTE_HTML;
use super::{RemoteCommand, RemoteCommandMsg};

pub struct RemoteServer;

impl RemoteServer {
    /// Start the WebSocket remote control server in a background thread.
    pub fn start(port: u16) -> (mpsc::Receiver<RemoteCommand>, broadcast::Sender<String>) {
        let (cmd_tx, cmd_rx) = mpsc::channel();
        let (state_tx, _) = broadcast::channel(64);
        let state_tx_clone = state_tx.clone();

        thread::spawn(move || {
            let rt = tokio::runtime::Runtime::new().expect("failed to create tokio runtime");
            rt.block_on(async move {
                let listener = TcpListener::bind(format!("127.0.0.1:{}", port))
                    .await
                    .expect("failed to bind remote control server");

                loop {
                    if let Ok((stream, _)) = listener.accept().await {
                        let cmd_tx = cmd_tx.clone();
                        let state_rx = state_tx_clone.subscribe();

                        tokio::spawn(async move {
                            handle_connection(stream, cmd_tx, state_rx).await;
                        });
                    }
                }
            });
        });

        (cmd_rx, state_tx)
    }
}

async fn handle_connection(
    mut stream: tokio::net::TcpStream,
    cmd_tx: mpsc::Sender<RemoteCommand>,
    state_rx: broadcast::Receiver<String>,
) {
    // Peek at first bytes to determine if this is a WebSocket upgrade or plain HTTP
    let mut buf = [0u8; 4096];
    let n = match stream.peek(&mut buf).await {
        Ok(n) => n,
        Err(_) => return,
    };

    let request = String::from_utf8_lossy(&buf[..n]);

    // Check if this is a WebSocket upgrade request
    if request.contains("Upgrade: websocket") || request.contains("upgrade: websocket") {
        // Validate Origin header for WebSocket connections (CSRF protection)
        let origin_ok = if let Some(origin_line) = request.lines()
            .find(|l| l.to_lowercase().starts_with("origin:"))
        {
            let origin = origin_line.split_once(':').map(|x| x.1).unwrap_or("").trim();
            if origin.is_empty() || origin.starts_with("file://") {
                true
            } else if let Ok(url) = url::Url::parse(origin) {
                matches!(url.host_str(), Some("127.0.0.1") | Some("localhost"))
            } else {
                false
            }
        } else {
            true // No Origin header = non-browser client, allow
        };

        if !origin_ok {
            let response = b"HTTP/1.1 403 Forbidden\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";
            let mut request_data = vec![0u8; n];
            let _ = stream.read(&mut request_data).await;
            let _ = stream.write_all(response).await;
            return;
        }

        let ws_stream = match tokio_tungstenite::accept_async(stream).await {
            Ok(ws) => ws,
            Err(_) => return,
        };
        handle_websocket(ws_stream, cmd_tx, state_rx).await;
    } else {
        // Serve HTML page with security headers
        let mut request_data = vec![0u8; n];
        let _ = stream.read(&mut request_data).await;

        let response = format!(
            "HTTP/1.1 200 OK\r\n\
             Content-Type: text/html; charset=utf-8\r\n\
             Content-Length: {}\r\n\
             Connection: close\r\n\
             X-Content-Type-Options: nosniff\r\n\
             X-Frame-Options: DENY\r\n\
             Content-Security-Policy: default-src 'self' 'unsafe-inline'; connect-src ws://127.0.0.1:* ws://localhost:*; style-src 'self' 'unsafe-inline' https://fonts.googleapis.com; font-src https://fonts.gstatic.com\r\n\
             \r\n{}",
            REMOTE_HTML.len(),
            REMOTE_HTML
        );
        let _ = stream.write_all(response.as_bytes()).await;
    }
}

async fn handle_websocket(
    ws_stream: tokio_tungstenite::WebSocketStream<tokio::net::TcpStream>,
    cmd_tx: mpsc::Sender<RemoteCommand>,
    mut state_rx: broadcast::Receiver<String>,
) {
    let (mut ws_sink, mut ws_stream_rx) = ws_stream.split();

    // Channel to forward state broadcasts to the ws_sink task
    let (fwd_tx, mut fwd_rx) = tokio::sync::mpsc::channel::<String>(64);

    // Task: forward broadcast state to this client
    let broadcast_task = tokio::spawn(async move {
        loop {
            match state_rx.recv().await {
                Ok(msg) => {
                    if fwd_tx.send(msg).await.is_err() {
                        break;
                    }
                }
                Err(broadcast::error::RecvError::Lagged(_)) => continue,
                Err(_) => break,
            }
        }
    });

    // Task: write forwarded messages to WebSocket sink
    let sink_task = tokio::spawn(async move {
        while let Some(msg) = fwd_rx.recv().await {
            if ws_sink.send(Message::Text(msg.into())).await.is_err() {
                break;
            }
        }
    });

    // Process incoming WebSocket messages
    while let Some(Ok(msg)) = ws_stream_rx.next().await {
        match msg {
            Message::Text(text) => {
                // Drop oversized messages to prevent OOM from malicious clients
                if text.len() > 4096 {
                    continue;
                }
                if let Ok(cmd_msg) = serde_json::from_str::<RemoteCommandMsg>(&text) {
                    if cmd_msg.msg_type == "command" {
                        let command = match cmd_msg.action.as_str() {
                            "next" => Some(RemoteCommand::Next),
                            "prev" => Some(RemoteCommand::Prev),
                            "goto" => cmd_msg.slide.map(RemoteCommand::Goto),
                            "next_section" => Some(RemoteCommand::NextSection),
                            "prev_section" => Some(RemoteCommand::PrevSection),
                            "scroll_up" => Some(RemoteCommand::ScrollUp),
                            "scroll_down" => Some(RemoteCommand::ScrollDown),
                            "toggle_fullscreen" => Some(RemoteCommand::ToggleFullscreen),
                            "toggle_notes" => Some(RemoteCommand::ToggleNotes),
                            "toggle_theme_name" => Some(RemoteCommand::ToggleThemeName),
                            "toggle_sections" => Some(RemoteCommand::ToggleSections),
                            "toggle_dark_mode" => Some(RemoteCommand::ToggleDarkMode),
                            "scale_up" => Some(RemoteCommand::ScaleUp),
                            "scale_down" => Some(RemoteCommand::ScaleDown),
                            "image_scale_up" => Some(RemoteCommand::ImageScaleUp),
                            "image_scale_down" => Some(RemoteCommand::ImageScaleDown),
                            "font_up" => Some(RemoteCommand::FontUp),
                            "font_down" => Some(RemoteCommand::FontDown),
                            "font_reset" => Some(RemoteCommand::FontReset),
                            "execute_code" => Some(RemoteCommand::ExecuteCode),
                            "timer_start" => Some(RemoteCommand::TimerStart),
                            "timer_reset" => Some(RemoteCommand::TimerReset),
                            _ => None,
                        };
                        if let Some(cmd) = command {
                            let _ = cmd_tx.send(cmd);
                        }
                    }
                }
            }
            Message::Close(_) => break,
            _ => {}
        }
    }

    broadcast_task.abort();
    sink_task.abort();
}
