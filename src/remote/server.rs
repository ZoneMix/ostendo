//! WebSocket server for remote presentation control.
//!
//! Listens on `127.0.0.1` only. A plain HTTP request gets the embedded control
//! page; a WebSocket upgrade is checked (Origin, optional `--remote-token`) and
//! then carries JSON commands inbound and state broadcasts outbound. The server
//! runs its own Tokio runtime on a background thread so it never blocks the
//! render loop.

use std::net::Ipv4Addr;
use std::sync::mpsc;
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use anyhow::{ensure, Context, Result};
use futures_util::{SinkExt, StreamExt};
use subtle::ConstantTimeEq;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{broadcast, Semaphore};
use tokio_tungstenite::tungstenite::handshake::server::{
    Callback, ErrorResponse, Request, Response,
};
use tokio_tungstenite::tungstenite::http::header::{AUTHORIZATION, ORIGIN, SEC_WEBSOCKET_PROTOCOL};
use tokio_tungstenite::tungstenite::http::{HeaderMap, HeaderValue, StatusCode};
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::WebSocketStream;

use super::html::REMOTE_HTML;
use super::{RemoteCommand, RemoteCommandMsg};

const MAX_CONNECTIONS: usize = 8;

/// Every connection holds a slot from the moment it is accepted, so an idle
/// socket that never finishes its request must be dropped or a handful of
/// them would lock the remote out for good.
const PRE_AUTH_TIMEOUT: Duration = Duration::from_secs(5);

const MAX_COMMAND_BYTES: usize = 4096;

/// Bind `127.0.0.1:port` and serve remote control on a background thread.
///
/// Returns the command receiver for the presenter and the sender it uses to
/// broadcast state JSON to all clients.
///
/// # Errors
///
/// Fails when the port cannot be bound or `token` is empty or contains
/// characters outside `[A-Za-z0-9._~-]`. The browser page sends the token as a
/// WebSocket subprotocol, which must be an HTTP token, and reads it back from
/// the URL fragment, where `+` decodes to a space.
pub fn start(
    port: u16,
    token: Option<String>,
) -> Result<(mpsc::Receiver<RemoteCommand>, broadcast::Sender<String>)> {
    if let Some(token) = &token {
        ensure!(
            !token.is_empty()
                && token
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"._~-".contains(&b)),
            "--remote-token may only contain A-Z a-z 0-9 . _ ~ -"
        );
    }
    let listener = std::net::TcpListener::bind((Ipv4Addr::LOCALHOST, port))
        .with_context(|| format!("cannot start remote control on 127.0.0.1:{port}"))?;
    spawn(listener, token)
}

fn spawn(
    listener: std::net::TcpListener,
    token: Option<String>,
) -> Result<(mpsc::Receiver<RemoteCommand>, broadcast::Sender<String>)> {
    listener.set_nonblocking(true)?;
    let runtime = tokio::runtime::Runtime::new()?;
    let listener = {
        let _guard = runtime.enter();
        TcpListener::from_std(listener)?
    };
    let (cmd_tx, cmd_rx) = mpsc::channel();
    let (state_tx, _) = broadcast::channel(64);
    let states = state_tx.clone();
    thread::spawn(move || runtime.block_on(accept_loop(listener, cmd_tx, states, token)));
    Ok((cmd_rx, state_tx))
}

async fn accept_loop(
    listener: TcpListener,
    cmd_tx: mpsc::Sender<RemoteCommand>,
    states: broadcast::Sender<String>,
    token: Option<String>,
) {
    let slots = Arc::new(Semaphore::new(MAX_CONNECTIONS));
    loop {
        let Ok((stream, _)) = listener.accept().await else {
            continue;
        };
        let Ok(permit) = slots.clone().try_acquire_owned() else {
            continue;
        };
        let cmd_tx = cmd_tx.clone();
        let state_rx = states.subscribe();
        let token = token.clone();
        tokio::spawn(async move {
            if let Ok(Some(ws)) =
                tokio::time::timeout(PRE_AUTH_TIMEOUT, accept(stream, token)).await
            {
                handle_websocket(ws, cmd_tx, state_rx).await;
            }
            drop(permit);
        });
    }
}

/// Serve the control page, or complete an authorized WebSocket handshake.
async fn accept(stream: TcpStream, token: Option<String>) -> Option<WebSocketStream<TcpStream>> {
    if !requests_websocket(&stream).await {
        serve_control_page(stream).await;
        return None;
    }
    tokio_tungstenite::accept_hdr_async(stream, Authorize { token })
        .await
        .ok()
}

/// Routing only: the peeked bytes stay in the socket for tungstenite, which
/// parses the full request and enforces auth in [`Authorize`].
async fn requests_websocket(stream: &TcpStream) -> bool {
    let mut buf = [0u8; 4096];
    let Ok(n) = stream.peek(&mut buf).await else {
        return false;
    };
    String::from_utf8_lossy(&buf[..n]).lines().any(|line| {
        line.split_once(':').is_some_and(|(name, value)| {
            name.trim().eq_ignore_ascii_case("upgrade")
                && value.to_ascii_lowercase().contains("websocket")
        })
    })
}

async fn serve_control_page(mut stream: TcpStream) {
    // Drain the request first: closing a socket with unread input sends a
    // reset, which can discard the response before the browser reads it.
    let mut request = [0u8; 4096];
    let _ = stream.read(&mut request).await;
    let response = format!(
        "HTTP/1.1 200 OK\r\n\
         Content-Type: text/html; charset=utf-8\r\n\
         Content-Length: {}\r\n\
         Connection: close\r\n\
         X-Content-Type-Options: nosniff\r\n\
         X-Frame-Options: DENY\r\n\
         Content-Security-Policy: default-src 'none'; script-src 'unsafe-inline'; connect-src ws://127.0.0.1:* ws://localhost:*; style-src 'unsafe-inline' https://fonts.googleapis.com; font-src https://fonts.gstatic.com; img-src data:\r\n\
         \r\n{}",
        REMOTE_HTML.len(),
        REMOTE_HTML
    );
    let _ = stream.write_all(response.as_bytes()).await;
}

/// Handshake gate: rejects foreign browser origins (403) and, when a token is
/// configured, requests that carry it neither as `Authorization: Bearer` nor
/// as an offered subprotocol (401).
struct Authorize {
    token: Option<String>,
}

impl Callback for Authorize {
    fn on_request(
        self,
        request: &Request,
        mut response: Response,
    ) -> Result<Response, ErrorResponse> {
        let headers = request.headers();
        if !origin_allowed(headers) {
            return Err(reject(StatusCode::FORBIDDEN));
        }
        let Some(token) = self.token else {
            return Ok(response);
        };
        if offered_subprotocols(headers).any(|p| secure_eq(p, &token)) {
            // Browsers can only send the token as a subprotocol and fail the
            // connection unless the server selects one of those offered.
            let selected =
                HeaderValue::from_str(&token).map_err(|_| reject(StatusCode::UNAUTHORIZED))?;
            response
                .headers_mut()
                .insert(SEC_WEBSOCKET_PROTOCOL, selected);
            return Ok(response);
        }
        if bearer_token(headers).is_some_and(|t| secure_eq(t, &token)) {
            return Ok(response);
        }
        Err(reject(StatusCode::UNAUTHORIZED))
    }
}

fn reject(status: StatusCode) -> ErrorResponse {
    let mut response = ErrorResponse::new(None);
    *response.status_mut() = status;
    response
}

/// Blocks cross-site WebSocket hijacking from browser pages. Clients that send
/// no Origin are not browsers and are left to token auth.
fn origin_allowed(headers: &HeaderMap) -> bool {
    let Some(origin) = headers.get(ORIGIN) else {
        return true;
    };
    let Ok(origin) = origin.to_str() else {
        return false;
    };
    origin.starts_with("file://")
        || url::Url::parse(origin)
            .is_ok_and(|url| matches!(url.host_str(), Some("127.0.0.1" | "localhost")))
}

fn offered_subprotocols(headers: &HeaderMap) -> impl Iterator<Item = &str> {
    headers
        .get_all(SEC_WEBSOCKET_PROTOCOL)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .flat_map(|value| value.split(',').map(str::trim))
}

fn bearer_token(headers: &HeaderMap) -> Option<&str> {
    let (scheme, token) = headers.get(AUTHORIZATION)?.to_str().ok()?.split_once(' ')?;
    scheme.eq_ignore_ascii_case("bearer").then(|| token.trim())
}

fn secure_eq(a: &str, b: &str) -> bool {
    a.as_bytes().ct_eq(b.as_bytes()).into()
}

/// Pump state broadcasts out and commands in until the client disconnects.
async fn handle_websocket(
    ws: WebSocketStream<TcpStream>,
    cmd_tx: mpsc::Sender<RemoteCommand>,
    mut state_rx: broadcast::Receiver<String>,
) {
    let (mut sink, mut incoming) = ws.split();

    let forward = tokio::spawn(async move {
        loop {
            match state_rx.recv().await {
                Ok(state) => {
                    if sink.send(Message::Text(state.into())).await.is_err() {
                        break;
                    }
                }
                Err(broadcast::error::RecvError::Lagged(_)) => continue,
                Err(broadcast::error::RecvError::Closed) => break,
            }
        }
    });

    while let Some(Ok(msg)) = incoming.next().await {
        match msg {
            Message::Text(text) if text.len() <= MAX_COMMAND_BYTES => {
                let command = serde_json::from_str::<RemoteCommandMsg>(&text)
                    .ok()
                    .and_then(RemoteCommandMsg::into_command);
                if let Some(command) = command {
                    let _ = cmd_tx.send(command);
                }
            }
            Message::Close(_) => break,
            _ => {}
        }
    }

    forward.abort();
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio_tungstenite::tungstenite::client::IntoClientRequest;
    use tokio_tungstenite::tungstenite::handshake::client::Response as ClientResponse;
    use tokio_tungstenite::tungstenite::Error as WsError;
    use tokio_tungstenite::MaybeTlsStream;

    const TOKEN: &str = "s3cret-token";

    fn serve(token: Option<&str>) -> (u16, mpsc::Receiver<RemoteCommand>) {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let (commands, _states) = spawn(listener, token.map(String::from)).unwrap();
        (port, commands)
    }

    async fn connect(
        port: u16,
        headers: &[(&'static str, &str)],
    ) -> Result<(WebSocketStream<MaybeTlsStream<TcpStream>>, ClientResponse), WsError> {
        let mut request = format!("ws://127.0.0.1:{port}/")
            .into_client_request()
            .unwrap();
        for (name, value) in headers {
            request.headers_mut().insert(*name, value.parse().unwrap());
        }
        tokio_tungstenite::connect_async(request).await
    }

    fn status(result: Result<impl Sized, WsError>) -> Option<StatusCode> {
        match result {
            Err(WsError::Http(response)) => Some(response.status()),
            _ => None,
        }
    }

    #[test]
    fn start_rejects_tokens_the_browser_page_cannot_send() {
        for token in ["", "a+b", "a/b=", "a b"] {
            assert!(
                start(0, Some(token.to_string())).is_err(),
                "{token:?} accepted"
            );
        }
    }

    #[test]
    fn start_reports_a_port_that_is_in_use() {
        let taken = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = taken.local_addr().unwrap().port();
        assert!(start(port, None).is_err());
    }

    #[tokio::test]
    async fn serves_control_page_over_plain_http() {
        let (port, _commands) = serve(Some(TOKEN));
        let mut stream = TcpStream::connect(("127.0.0.1", port)).await.unwrap();
        stream
            .write_all(b"GET / HTTP/1.1\r\nHost: localhost\r\n\r\n")
            .await
            .unwrap();
        let mut body = String::new();
        stream.read_to_string(&mut body).await.unwrap();
        assert!(body.starts_with("HTTP/1.1 200 OK"));
        assert!(body.contains("<title>Ostendo Remote</title>"));
    }

    #[tokio::test]
    async fn token_is_required_when_configured() {
        let (port, _commands) = serve(Some(TOKEN));
        assert_eq!(
            status(connect(port, &[]).await),
            Some(StatusCode::UNAUTHORIZED)
        );
        let wrong = [("authorization", "Bearer wrong")];
        assert_eq!(
            status(connect(port, &wrong).await),
            Some(StatusCode::UNAUTHORIZED)
        );
    }

    #[tokio::test]
    async fn bearer_token_connects_without_selecting_a_subprotocol() {
        let (port, commands) = serve(Some(TOKEN));
        let bearer = format!("bearer {TOKEN}");
        let (mut ws, response) = connect(port, &[("authorization", &bearer)]).await.unwrap();
        assert!(response.headers().get(SEC_WEBSOCKET_PROTOCOL).is_none());

        let goto = r#"{"type":"command","action":"goto","slide":5}"#;
        ws.send(Message::Text(goto.into())).await.unwrap();
        let command =
            tokio::task::spawn_blocking(move || commands.recv_timeout(Duration::from_secs(5)))
                .await
                .unwrap();
        assert!(matches!(command, Ok(RemoteCommand::Goto(5))), "{command:?}");
    }

    #[tokio::test]
    async fn subprotocol_token_is_selected_for_browsers() {
        let (port, _commands) = serve(Some(TOKEN));
        let offered = format!("other, {TOKEN}");
        let (_ws, response) = connect(port, &[("sec-websocket-protocol", &offered)])
            .await
            .unwrap();
        assert_eq!(response.headers()[SEC_WEBSOCKET_PROTOCOL], TOKEN);
    }

    #[tokio::test]
    async fn rejects_foreign_browser_origin() {
        let (port, _commands) = serve(None);
        let foreign = [("origin", "http://evil.example")];
        assert_eq!(
            status(connect(port, &foreign).await),
            Some(StatusCode::FORBIDDEN)
        );
        assert!(connect(port, &[("origin", "http://localhost:8765")])
            .await
            .is_ok());
    }

    #[tokio::test]
    async fn idle_connections_cannot_hold_the_connection_cap() {
        let (port, _commands) = serve(None);
        let mut idle = Vec::new();
        for _ in 0..MAX_CONNECTIONS {
            idle.push(TcpStream::connect(("127.0.0.1", port)).await.unwrap());
        }
        assert!(
            connect(port, &[]).await.is_err(),
            "connection cap not enforced"
        );

        tokio::time::sleep(PRE_AUTH_TIMEOUT + Duration::from_millis(500)).await;
        assert!(
            connect(port, &[]).await.is_ok(),
            "idle sockets still hold every slot"
        );
    }
}
