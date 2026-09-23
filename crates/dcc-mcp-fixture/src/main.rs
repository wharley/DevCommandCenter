use std::{env, net::SocketAddr, path::PathBuf, process::ExitCode};

use dcc_mcp_fixture::{
    http::serve_http, stdio::serve_stdio, FixtureServer, COMPUTER_USE_IMAGE_LABELS,
};
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
        Some("stdio") => {
            let (image_case, hold_start_file) = parse_fixture_args(arguments)?;
            serve_stdio(
                tokio::io::stdin(),
                tokio::io::stdout(),
                FixtureServer::with_computer_use_fixture(image_case, hold_start_file),
            )
            .await
            .map_err(|error| error.to_string())
        }
        Some("http") => {
            let (bind, image_case, hold_start_file) = parse_http_args(arguments)?;
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
            serve_http(
                listener,
                FixtureServer::with_computer_use_fixture(image_case, hold_start_file),
            )
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

fn parse_http_args(
    mut arguments: impl Iterator<Item = String>,
) -> Result<(SocketAddr, usize, Option<PathBuf>), String> {
    let mut bind = DEFAULT_HTTP_BIND.to_string();
    let mut image_case = 0;
    let mut hold_start_file = None;
    while let Some(argument) = arguments.next() {
        match argument.as_str() {
            "--bind" => {
                bind = arguments
                    .next()
                    .ok_or_else(|| "--bind requires an address".to_string())?;
            }
            "--image-case" => {
                let value = arguments
                    .next()
                    .ok_or_else(|| "--image-case requires a number".to_string())?;
                image_case = parse_image_case_value(&value)?;
            }
            "--hold-start-file" => {
                let path = arguments
                    .next()
                    .ok_or_else(|| "--hold-start-file requires a path".to_string())?;
                hold_start_file = Some(PathBuf::from(path));
            }
            _ => return Err(format!("unknown HTTP argument: {argument}")),
        }
    }
    let bind = bind
        .parse()
        .map_err(|_| format!("invalid HTTP bind address: {bind}"))?;
    Ok((bind, image_case, hold_start_file))
}

fn parse_fixture_args(
    mut arguments: impl Iterator<Item = String>,
) -> Result<(usize, Option<PathBuf>), String> {
    let mut image_case = 0;
    let mut hold_start_file = None;
    while let Some(argument) = arguments.next() {
        match argument.as_str() {
            "--image-case" => {
                let value = arguments
                    .next()
                    .ok_or_else(|| "--image-case requires a number".to_string())?;
                image_case = parse_image_case_value(&value)?;
            }
            "--hold-start-file" => {
                let path = arguments
                    .next()
                    .ok_or_else(|| "--hold-start-file requires a path".to_string())?;
                hold_start_file = Some(PathBuf::from(path));
            }
            _ => return Err(format!("unknown stdio argument: {argument}")),
        }
    }
    Ok((image_case, hold_start_file))
}

fn parse_image_case_value(value: &str) -> Result<usize, String> {
    let case = value
        .parse::<usize>()
        .map_err(|_| "image case must be a non-negative integer".to_string())?;
    if case >= COMPUTER_USE_IMAGE_LABELS.len() {
        return Err("image case is out of range".to_string());
    }
    Ok(case)
}

fn print_help() {
    eprintln!(
        "DCC offline MCP fixture\n\n\
         Usage:\n  \
         dcc-mcp-fixture stdio [--image-case 0..3] [--hold-start-file PATH]\n  \
         dcc-mcp-fixture http [--bind 127.0.0.1:8765] [--image-case 0..3] [--hold-start-file PATH]"
    );
}
