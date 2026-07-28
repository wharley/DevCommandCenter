use std::{path::PathBuf, process::Stdio, sync::Arc, time::Duration};

use dcc_core::{
    application::probe_mcp_definition,
    domain::mcp::{
        McpDefinition, McpDefinitionId, McpDefinitionOwnership, McpErrorCategory, McpSecretBinding,
        McpSecretReferenceId, McpSecretTarget, McpTransport, McpTrust, McpTrustDecision,
        McpTrustFingerprint,
    },
};
use dcc_infra::{credential_store::InMemoryCredentialStore, mcp_probe::SecureMcpProbe};
use tokio::{
    io::{AsyncBufReadExt, BufReader},
    process::{Child, Command},
    time::timeout,
};

fn fixture_binary() -> PathBuf {
    std::fs::canonicalize(env!("CARGO_BIN_EXE_dcc-mcp-fixture")).expect("canonical fixture binary")
}

fn trusted_definition(transport: McpTransport) -> McpDefinition {
    let mut definition = McpDefinition {
        id: McpDefinitionId("offline-fixture".to_string()),
        display_name: "Offline fixture".to_string(),
        transport,
        secret_refs: Vec::new(),
        enabled: false,
        ownership: McpDefinitionOwnership::DccManaged,
        trust: McpTrust {
            current_fingerprint: McpTrustFingerprint("0".repeat(64)),
            decision: McpTrustDecision::Untrusted,
        },
        created_at: "2026-07-28T00:00:00Z".to_string(),
        updated_at: "2026-07-28T00:00:00Z".to_string(),
    };
    definition.synchronize_trust_fingerprint();
    definition.trust.decision = McpTrustDecision::Trusted {
        fingerprint: definition.trust.current_fingerprint.clone(),
        trusted_at: "2026-07-28T00:00:00Z".to_string(),
    };
    definition
}

fn probe() -> SecureMcpProbe<InMemoryCredentialStore> {
    SecureMcpProbe::new(Arc::new(InMemoryCredentialStore::default())).expect("create secure probe")
}

fn assert_fixture_report(report: dcc_core::domain::mcp::McpProbeReport) {
    assert_eq!(report.protocol_version, "2025-11-25");
    let names = report
        .tools
        .iter()
        .map(|tool| tool.name.as_str())
        .collect::<Vec<_>>();
    assert_eq!(
        names,
        vec![
            "fixture.echo",
            "fixture.fail",
            "fixture.malformed_result",
            "fixture.mutate",
            "fixture.slow"
        ]
    );
}

#[tokio::test]
async fn secure_probe_initializes_and_lists_tools_over_stdio() {
    let cwd =
        std::fs::canonicalize(env!("CARGO_MANIFEST_DIR")).expect("canonical working directory");
    let definition = trusted_definition(McpTransport::Stdio {
        executable: fixture_binary().to_string_lossy().to_string(),
        args: vec!["stdio".to_string()],
        cwd: Some(cwd.to_string_lossy().to_string()),
    });

    let report = probe_mcp_definition(&probe(), &definition)
        .await
        .expect("probe stdio fixture");
    assert_fixture_report(report);
}

#[tokio::test]
async fn secure_probe_normalizes_missing_credentials_without_exposing_references() {
    let cwd =
        std::fs::canonicalize(env!("CARGO_MANIFEST_DIR")).expect("canonical working directory");
    let mut definition = trusted_definition(McpTransport::Stdio {
        executable: fixture_binary().to_string_lossy().to_string(),
        args: vec!["stdio".to_string()],
        cwd: Some(cwd.to_string_lossy().to_string()),
    });
    definition.secret_refs.push(McpSecretBinding {
        target: McpSecretTarget::EnvironmentVariable {
            name: "PRIVATE_PAYMENT_TOKEN".to_string(),
        },
        secret_ref: McpSecretReferenceId("credential:super-sensitive".to_string()),
    });
    definition.synchronize_trust_fingerprint();
    definition.trust.decision = McpTrustDecision::Trusted {
        fingerprint: definition.trust.current_fingerprint.clone(),
        trusted_at: "2026-07-28T00:00:00Z".to_string(),
    };

    let error = probe_mcp_definition(&probe(), &definition)
        .await
        .expect_err("missing credential");
    assert_eq!(error.category, McpErrorCategory::Authentication);
    assert!(!error.message.contains("PRIVATE_PAYMENT_TOKEN"));
    assert!(!error.message.contains("super-sensitive"));
}

#[tokio::test]
async fn secure_probe_initializes_and_lists_tools_over_streamable_http() {
    let (mut fixture, url) = start_http_fixture().await;
    let definition = trusted_definition(McpTransport::Http { url });

    let report = probe_mcp_definition(&probe(), &definition)
        .await
        .expect("probe HTTP fixture");
    assert_fixture_report(report);

    let _ = fixture.start_kill();
    let _ = timeout(Duration::from_secs(1), fixture.wait()).await;
}

async fn start_http_fixture() -> (Child, String) {
    let mut command = Command::new(fixture_binary());
    command
        .arg("http")
        .arg("--bind")
        .arg("127.0.0.1:0")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    let mut child = command.spawn().expect("start HTTP fixture");
    let stderr = child.stderr.take().expect("fixture stderr");
    let mut lines = BufReader::new(stderr).lines();
    let endpoint = timeout(Duration::from_secs(2), lines.next_line())
        .await
        .expect("fixture startup timeout")
        .expect("read fixture endpoint")
        .expect("fixture endpoint line");
    let url = endpoint
        .strip_prefix("DCC_MCP_FIXTURE_URL=")
        .expect("fixture endpoint prefix")
        .to_string();
    (child, url)
}
