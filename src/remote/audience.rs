//! `--audience`: a page where everyone in the room votes on the poll on
//! screen. It listens on every interface, so it can do nothing else: no
//! presenter controls, no notes, only the poll and one vote per connection
//! for each poll.

use std::collections::HashSet;
use std::net::{Ipv4Addr, UdpSocket};
use std::sync::mpsc;
use std::sync::Arc;

use anyhow::{Context, Result};
use futures_util::{SinkExt, StreamExt};
use serde::Deserialize;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{watch, Semaphore};
use tokio_tungstenite::tungstenite::handshake::server::{
    Callback, ErrorResponse, Request, Response,
};
use tokio_tungstenite::tungstenite::http::header::{HOST, ORIGIN};
use tokio_tungstenite::tungstenite::http::StatusCode;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::WebSocketStream;

use super::server::{header, peek_head, reject, run_in_background, serve_page, PRE_AUTH_TIMEOUT};
use super::RemoteCommand;

const VOTE_HTML: &str = include_str!("vote.html");

/// Phones that can have the page open at once.
const MAX_VOTERS: usize = 256;

const MAX_VOTE_BYTES: usize = 256;

/// Serves the voting page on every interface at `port`, sending votes to
/// `commands`. Returns the sender for the poll JSON pages show, and the
/// page's address on this machine's network.
///
/// # Errors
/// Fails when the port cannot be bound.
pub fn start(
    port: u16,
    commands: mpsc::Sender<RemoteCommand>,
) -> Result<(watch::Sender<String>, String)> {
    let listener = std::net::TcpListener::bind((Ipv4Addr::UNSPECIFIED, port))
        .with_context(|| format!("cannot start the audience page on port {port}"))?;
    let polls = spawn(listener, commands)?;
    Ok((polls, format!("http://{}:{port}/", lan_address())))
}

fn spawn(
    listener: std::net::TcpListener,
    commands: mpsc::Sender<RemoteCommand>,
) -> Result<watch::Sender<String>> {
    let (polls, _) = watch::channel(r#"{"type":"poll"}"#.to_string());
    let sender = polls.clone();
    run_in_background(listener, move |listener| {
        accept_loop(listener, commands, sender)
    })?;
    Ok(polls)
}

/// The address other machines reach this one on. Connecting a UDP socket
/// sends nothing; it only picks the interface a packet out would leave from.
fn lan_address() -> std::net::IpAddr {
    UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0))
        .and_then(|socket| {
            socket.connect((Ipv4Addr::new(192, 0, 2, 1), 9))?;
            socket.local_addr()
        })
        .map_or(Ipv4Addr::LOCALHOST.into(), |addr| addr.ip())
}

async fn accept_loop(
    listener: TcpListener,
    commands: mpsc::Sender<RemoteCommand>,
    polls: watch::Sender<String>,
) {
    let slots = Arc::new(Semaphore::new(MAX_VOTERS));
    loop {
        let Ok((stream, _)) = listener.accept().await else {
            continue;
        };
        let Ok(permit) = slots.clone().try_acquire_owned() else {
            continue;
        };
        let commands = commands.clone();
        let polls = polls.subscribe();
        tokio::spawn(async move {
            if let Ok(Some(ws)) = tokio::time::timeout(PRE_AUTH_TIMEOUT, accept(stream)).await {
                handle_voter(ws, commands, polls).await;
            }
            drop(permit);
        });
    }
}

async fn accept(stream: TcpStream) -> Option<WebSocketStream<TcpStream>> {
    let head = peek_head(&stream).await;
    if header(&head, "upgrade").is_none() {
        // The page may only talk back to the host it came from.
        let host = header(&head, "host")
            .filter(|h| {
                h.chars()
                    .all(|c| c.is_ascii_alphanumeric() || ".:-[]".contains(c))
            })
            .unwrap_or("127.0.0.1");
        let csp = format!(
            "default-src 'none'; script-src 'unsafe-inline'; \
             style-src 'unsafe-inline'; connect-src ws://{host}"
        );
        serve_page(stream, VOTE_HTML, &csp).await;
        return None;
    }
    tokio_tungstenite::accept_hdr_async(stream, SameOrigin)
        .await
        .ok()
}

/// Only the voting page itself may connect: a browser's Origin must name the
/// host it asked for, so another site cannot stuff the ballot from its
/// visitors' browsers.
struct SameOrigin;

impl Callback for SameOrigin {
    fn on_request(self, request: &Request, response: Response) -> Result<Response, ErrorResponse> {
        let headers = request.headers();
        let Some(origin) = headers.get(ORIGIN) else {
            return Ok(response);
        };
        let host = headers.get(HOST).and_then(|h| h.to_str().ok());
        let origin_host = origin.to_str().ok().and_then(|o| o.strip_prefix("http://"));
        match (origin_host, host) {
            (Some(o), Some(h)) if o.eq_ignore_ascii_case(h) => Ok(response),
            _ => Err(reject(StatusCode::FORBIDDEN)),
        }
    }
}

#[derive(Deserialize)]
struct Vote {
    #[serde(rename = "type")]
    kind: String,
    poll: String,
    option: usize,
}

async fn handle_voter(
    ws: WebSocketStream<TcpStream>,
    commands: mpsc::Sender<RemoteCommand>,
    mut polls: watch::Receiver<String>,
) {
    let (mut sink, mut incoming) = ws.split();
    let forward = tokio::spawn(async move {
        loop {
            let poll = polls.borrow_and_update().clone();
            if sink.send(Message::Text(poll.into())).await.is_err() {
                break;
            }
            if polls.changed().await.is_err() {
                break;
            }
        }
    });

    let mut voted = HashSet::new();
    while let Some(Ok(msg)) = incoming.next().await {
        match msg {
            Message::Text(text) if text.len() <= MAX_VOTE_BYTES => {
                let Ok(vote) = serde_json::from_str::<Vote>(&text) else {
                    continue;
                };
                if vote.kind == "vote" && voted.insert(vote.poll.clone()) {
                    let _ = commands.send(RemoteCommand::Vote {
                        poll: vote.poll,
                        option: vote.option,
                    });
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
    use std::time::Duration;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio_tungstenite::tungstenite::client::IntoClientRequest;
    use tokio_tungstenite::tungstenite::Error as WsError;

    #[tokio::test]
    async fn voters_see_the_poll_and_vote_once_per_poll() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let (tx, commands) = mpsc::channel();
        let polls = spawn(listener, tx).unwrap();
        polls.send_replace(r#"{"type":"poll","id":"p1"}"#.to_string());

        let mut page = TcpStream::connect(("127.0.0.1", port)).await.unwrap();
        page.write_all(b"GET / HTTP/1.1\r\nHost: 10.0.0.5:8766\r\n\r\n")
            .await
            .unwrap();
        let mut body = String::new();
        page.read_to_string(&mut body).await.unwrap();
        assert!(
            body.contains("connect-src ws://10.0.0.5:8766\r\n"),
            "{body}"
        );

        let mut foreign = format!("ws://127.0.0.1:{port}/")
            .into_client_request()
            .unwrap();
        foreign
            .headers_mut()
            .insert("origin", "http://evil.example".parse().unwrap());
        let refused = tokio_tungstenite::connect_async(foreign).await;
        assert!(
            matches!(&refused, Err(WsError::Http(r)) if r.status() == StatusCode::FORBIDDEN),
            "{refused:?}"
        );

        let (mut ws, _) = tokio_tungstenite::connect_async(format!("ws://127.0.0.1:{port}/"))
            .await
            .unwrap();
        let first = ws.next().await.unwrap().unwrap();
        assert_eq!(first.to_text().unwrap(), r#"{"type":"poll","id":"p1"}"#);
        for msg in [
            r#"{"type":"command","action":"next"}"#,
            r#"{"type":"vote","poll":"p1","option":2}"#,
            r#"{"type":"vote","poll":"p1","option":0}"#,
            r#"{"type":"vote","poll":"p2","option":1}"#,
        ] {
            ws.send(Message::Text(msg.into())).await.unwrap();
        }
        let received: Vec<String> = tokio::task::spawn_blocking(move || {
            std::iter::from_fn(|| commands.recv_timeout(Duration::from_secs(2)).ok())
                .map(|c| format!("{c:?}"))
                .collect()
        })
        .await
        .unwrap();
        assert_eq!(
            received,
            [
                r#"Vote { poll: "p1", option: 2 }"#,
                r#"Vote { poll: "p2", option: 1 }"#
            ]
        );
    }
}
