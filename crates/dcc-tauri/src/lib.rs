pub(crate) mod antigravity_account_state;
pub mod antigravity_installation;
pub mod commands;
pub(crate) mod delegation_apply;
pub mod delivery_failure;
pub mod events;
pub mod git;
pub mod guarded_undo_runtime;
pub mod process_runtime_registry;
pub mod run;
pub mod state;
pub mod terminal_arbiter;
pub mod turn_review;
pub mod workspace_setup;

// Capture-v2 adapters are intentionally compile-time gated and are not part
// of the terminal lifecycle yet. Keep their thread-safety contract explicit
// without coupling the process runtime to adapter internals.
#[cfg(all(target_os = "macos", feature = "guarded-undo-capture-v2"))]
#[allow(dead_code)]
fn _capture_v2_send_sync_assertions() {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<dcc_infra::guarded_undo::macos_root::MacWorkspaceRoot>();
    assert_send_sync::<dcc_infra::guarded_undo::macos_store::MacArtifactStoreLease>();
    assert_send_sync::<dcc_infra::guarded_undo::macos_store::MacArtifactStore>();
}

pub const PHASE_0A_CONTRACTS_DIR: &str = "../../packages/contracts/src/generated/bindings.ts";
