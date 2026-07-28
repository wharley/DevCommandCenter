use serde_json::Value;
use thiserror::Error;
use tokio::{
    io::{AsyncBufReadExt, AsyncRead, AsyncWrite, AsyncWriteExt, BufReader},
    sync::mpsc,
    task::JoinSet,
};

use crate::FixtureServer;

const MAX_STDIO_MESSAGE_BYTES: usize = 64 * 1024;

#[derive(Debug, Error)]
pub enum StdioFixtureError {
    #[error("stdio I/O failed")]
    Io(#[from] std::io::Error),
    #[error("stdio response serialization failed")]
    Serialization(#[from] serde_json::Error),
    #[error("stdio writer task failed")]
    WriterTask(#[from] tokio::task::JoinError),
}

pub async fn serve_stdio<R, W>(
    reader: R,
    writer: W,
    server: FixtureServer,
) -> Result<(), StdioFixtureError>
where
    R: AsyncRead + Unpin,
    W: AsyncWrite + Unpin + Send + 'static,
{
    let (responses, mut response_queue) = mpsc::channel::<Value>(32);
    let mut notifications = server.subscribe();
    let notification_responses = responses.clone();
    let notification_task = tokio::spawn(async move {
        loop {
            match notifications.recv().await {
                Ok(notification) => {
                    if notification_responses.send(notification).await.is_err() {
                        break;
                    }
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
            }
        }
    });
    let writer_task = tokio::spawn(async move {
        let mut writer = writer;
        while let Some(response) = response_queue.recv().await {
            let encoded = serde_json::to_vec(&response)?;
            writer.write_all(&encoded).await?;
            writer.write_all(b"\n").await?;
            writer.flush().await?;
        }
        Ok::<_, StdioFixtureError>(())
    });

    let mut lines = BufReader::new(reader).lines();
    let mut requests = JoinSet::new();
    while let Some(line) = lines.next_line().await? {
        if line.len() > MAX_STDIO_MESSAGE_BYTES {
            responses
                .send(parse_error())
                .await
                .map_err(|_| broken_pipe())?;
            continue;
        }

        let message = match serde_json::from_str::<Value>(&line) {
            Ok(message) => message,
            Err(_) => {
                responses
                    .send(parse_error())
                    .await
                    .map_err(|_| broken_pipe())?;
                continue;
            }
        };

        if message.get("id").is_some() || message.get("method").is_none() {
            let server = server.clone();
            let responses = responses.clone();
            requests.spawn(async move {
                if let Some(response) = server.handle_message(message).await {
                    let _ = responses.send(response).await;
                }
            });
        } else if let Some(response) = server.handle_message(message).await {
            responses.send(response).await.map_err(|_| broken_pipe())?;
        }
    }

    while requests.join_next().await.is_some() {}
    notification_task.abort();
    let _ = notification_task.await;
    drop(responses);
    writer_task.await??;
    Ok(())
}

fn parse_error() -> Value {
    serde_json::json!({
        "jsonrpc": "2.0",
        "id": null,
        "error": {
            "code": -32700,
            "message": "Parse error"
        }
    })
}

fn broken_pipe() -> StdioFixtureError {
    StdioFixtureError::Io(std::io::Error::new(
        std::io::ErrorKind::BrokenPipe,
        "stdio response channel closed",
    ))
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use serde_json::json;
    use tokio::io::{duplex, split, AsyncBufReadExt, AsyncWriteExt, BufReader};

    use super::*;

    #[tokio::test]
    async fn stdio_uses_newline_delimited_json_rpc_without_extra_output() {
        let (client, fixture) = duplex(16 * 1024);
        let (client_read, mut client_write) = split(client);
        let (fixture_read, fixture_write) = split(fixture);
        let task = tokio::spawn(serve_stdio(
            fixture_read,
            fixture_write,
            FixtureServer::new(),
        ));

        let request = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {
                "protocolVersion": "2025-11-25",
                "capabilities": {},
                "clientInfo": { "name": "test", "version": "1" }
            }
        });
        client_write
            .write_all(format!("{request}\n").as_bytes())
            .await
            .expect("write request");

        let mut response = String::new();
        tokio::time::timeout(
            Duration::from_secs(1),
            BufReader::new(client_read).read_line(&mut response),
        )
        .await
        .expect("response timeout")
        .expect("read response");
        let response: Value = serde_json::from_str(response.trim()).expect("JSON response");
        assert_eq!(response["id"], 1);
        assert_eq!(response["result"]["serverInfo"]["name"], "dcc-mcp-fixture");

        drop(client_write);
        task.await
            .expect("stdio task")
            .expect("stdio server completion");
    }

    #[tokio::test]
    async fn stdio_reports_parse_errors_as_json_rpc() {
        let (client, fixture) = duplex(16 * 1024);
        let (client_read, mut client_write) = split(client);
        let (fixture_read, fixture_write) = split(fixture);
        let task = tokio::spawn(serve_stdio(
            fixture_read,
            fixture_write,
            FixtureServer::new(),
        ));

        client_write
            .write_all(b"{not-json}\n")
            .await
            .expect("write malformed request");
        let mut response = String::new();
        BufReader::new(client_read)
            .read_line(&mut response)
            .await
            .expect("read response");
        let response: Value = serde_json::from_str(response.trim()).expect("JSON response");
        assert_eq!(response["error"]["code"], -32700);

        drop(client_write);
        task.await
            .expect("stdio task")
            .expect("stdio server completion");
    }

    #[tokio::test]
    async fn stdio_forwards_tool_list_change_notifications() {
        let (client, fixture) = duplex(16 * 1024);
        let (client_read, mut client_write) = split(client);
        let (fixture_read, fixture_write) = split(fixture);
        let task = tokio::spawn(serve_stdio(
            fixture_read,
            fixture_write,
            FixtureServer::new(),
        ));
        let mut client_read = BufReader::new(client_read);

        let mutation = json!({
            "jsonrpc": "2.0",
            "id": 7,
            "method": "tools/call",
            "params": {
                "name": "fixture.mutate",
                "arguments": { "changeTools": true }
            }
        });
        client_write
            .write_all(format!("{mutation}\n").as_bytes())
            .await
            .expect("write mutation");

        let mut messages = Vec::new();
        for _ in 0..2 {
            let mut line = String::new();
            tokio::time::timeout(Duration::from_secs(1), client_read.read_line(&mut line))
                .await
                .expect("message timeout")
                .expect("read message");
            messages.push(serde_json::from_str::<Value>(line.trim()).expect("JSON message"));
        }

        assert!(messages
            .iter()
            .any(|message| message["method"] == "notifications/tools/list_changed"));
        assert!(messages.iter().any(|message| message["id"] == 7));

        drop(client_write);
        drop(client_read);
        task.await
            .expect("stdio task")
            .expect("stdio server completion");
    }
}
