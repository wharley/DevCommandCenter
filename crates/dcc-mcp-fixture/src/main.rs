use std::{env, net::SocketAddr, process::ExitCode};

use dcc_mcp_fixture::{http::serve_http, stdio::serve_stdio, FixtureServer};
use tokio::net::TcpListener;

const DEFAULT_HTTP_BIND: &str = "127.0.0.1:8765";

#[tokio::main]
async fn main() -> ExitCode {
    match run().await {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("dcc-mcp-fixture: {error}");
            ExitCode::FAILURE
        }
    }
}

async fn run() -> Result<(), String> {
    let mut arguments = env::args().skip(1);
    match arguments.next().as_deref() {
        Some("stdio") if arguments.next().is_none() => serve_stdio(
            tokio::io::stdin(),
            tokio::io::stdout(),
            FixtureServer::new(),
        )
        .await
        .map_err(|error| error.to_string()),
        Some("http") => {
            let bind = parse_http_bind(arguments)?;
            if !bind.ip().is_loopback() {
                return Err("HTTP bind address must be loopback".to_string());
            }
            let listener = TcpListener::bind(bind)
                .await
                .map_err(|error| format!("failed to bind HTTP fixture: {error}"))?;
            let address = listener
                .local_addr()
                .map_err(|error| format!("failed to inspect HTTP fixture address: {error}"))?;
            eprintln!("DCC_MCP_FIXTURE_URL=http://{address}/mcp");
            serve_http(listener, FixtureServer::new())
                .await
                .map_err(|error| error.to_string())
        }
        Some("-h" | "--help") => {
            print_help();
            Ok(())
        }
        _ => {
            print_help();
            Err("expected `stdio` or `http` transport".to_string())
        }
    }
}

fn parse_http_bind(mut arguments: impl Iterator<Item = String>) -> Result<SocketAddr, String> {
    let mut bind = DEFAULT_HTTP_BIND.to_string();
    while let Some(argument) = arguments.next() {
        match argument.as_str() {
            "--bind" => {
                bind = arguments
                    .next()
                    .ok_or_else(|| "--bind requires an address".to_string())?;
            }
            _ => return Err(format!("unknown HTTP argument: {argument}")),
        }
    }
    bind.parse()
        .map_err(|_| format!("invalid HTTP bind address: {bind}"))
}

fn print_help() {
    eprintln!(
        "DCC offline MCP fixture\n\n\
         Usage:\n  \
         dcc-mcp-fixture stdio\n  \
         dcc-mcp-fixture http [--bind 127.0.0.1:8765]"
    );
}
