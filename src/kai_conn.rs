use std::time::Duration;

use serde_json::Value;
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    net::TcpStream,
    sync::{broadcast, mpsc},
    time::sleep,
};
use tracing::{error, info, warn};

use crate::protocol::{now_ms, ServerFrame};

/// A command sent from a browser handler to the KAI connection task.
pub struct EvalCommand {
    pub src: String,
    /// Channel back to the browser handler that sent this eval.
    pub reply: mpsc::Sender<ServerFrame>,
}

/// Spawns the KAI connection task and returns channels it exposes.
///
/// - `eval_tx`: browser handlers send `EvalCommand` here
/// - `sup_tx`: SUP broadcasts; handlers subscribe with `sup_tx.subscribe()`
pub async fn spawn(
    kai_addr: String,
    sup_tx: broadcast::Sender<ServerFrame>,
) -> mpsc::Sender<EvalCommand> {
    let (eval_tx, eval_rx) = mpsc::channel::<EvalCommand>(64);
    tokio::spawn(run(kai_addr, eval_rx, sup_tx));
    eval_tx
}

async fn run(
    kai_addr: String,
    mut eval_rx: mpsc::Receiver<EvalCommand>,
    sup_tx: broadcast::Sender<ServerFrame>,
) {
    let mut backoff = Duration::from_secs(1);

    loop {
        info!("Connecting to KAI at {}", kai_addr);
        match TcpStream::connect(&kai_addr).await {
            Ok(stream) => {
                info!("Connected to KAI at {}", kai_addr);
                backoff = Duration::from_secs(1);
                handle_connection(stream, &mut eval_rx, &sup_tx).await;
                warn!("KAI connection lost, reconnecting in {:?}", backoff);
            }
            Err(e) => {
                error!("Failed to connect to KAI: {e}, retrying in {:?}", backoff);
            }
        }

        sleep(backoff).await;
        backoff = (backoff * 2).min(Duration::from_secs(30));
    }
}

async fn handle_connection(
    stream: TcpStream,
    eval_rx: &mut mpsc::Receiver<EvalCommand>,
    sup_tx: &broadcast::Sender<ServerFrame>,
) {
    let (reader, mut writer) = stream.into_split();
    let mut lines = BufReader::new(reader).lines();

    // One pending eval at a time (KAI responds synchronously per command).
    let mut pending: Option<(String, mpsc::Sender<ServerFrame>)> = None;

    loop {
        tokio::select! {
            // Inbound line from KAI
            result = lines.next_line() => {
                match result {
                    Ok(Some(line)) => {
                        let line = line.trim().to_string();
                        if line.is_empty() { continue; }

                        if let Some(frame) = parse_sup(&line) {
                            let _ = sup_tx.send(frame);
                        } else if let Some((src, reply_tx)) = pending.take() {
                            let frame = ServerFrame::Result {
                                src,
                                output: line,
                                ts: now_ms(),
                            };
                            let _ = reply_tx.send(frame).await;
                        } else {
                            tracing::debug!("Unsolicited KAI line: {line}");
                        }
                    }
                    Ok(None) => { warn!("KAI closed connection"); return; }
                    Err(e) => { error!("KAI read error: {e}"); return; }
                }
            }

            // Eval command from a browser handler
            cmd = eval_rx.recv() => {
                match cmd {
                    Some(EvalCommand { src, reply }) => {
                        if let Err(e) = writer.write_all(format!("{src}\n").as_bytes()).await {
                            error!("KAI write error: {e}");
                            let _ = reply.send(ServerFrame::Error {
                                msg: format!("KAI write error: {e}"),
                                ts: now_ms(),
                            }).await;
                            return;
                        }
                        pending = Some((src, reply));
                    }
                    None => return, // all senders dropped
                }
            }
        }
    }
}

/// Parse `SUP node:reg:# <json>` into a ServerFrame::Sup.
pub(crate) fn parse_sup(line: &str) -> Option<ServerFrame> {
    let rest = line.strip_prefix("SUP ")?;
    let (addr, json_str) = rest.split_once(' ')?;
    let state: Value = serde_json::from_str(json_str).ok()?;
    Some(ServerFrame::Sup {
        addr: addr.to_string(),
        state,
        ts: now_ms(),
    })
}
