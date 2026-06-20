use std::{fs, path::PathBuf};

use chrono::Utc;
use sha2::{Digest, Sha256};

use crate::{
    commands::workspace_support::{
        resolve_branch_diff_base, resolve_current_branch_name, resolve_current_commit_sha,
    },
    git::{run_git_output, split_null_terminated_fields},
};

use super::{CodeRabbitDiffFingerprint, CodeRabbitReviewType};

pub(crate) fn build_diff_fingerprint(
    workspace_root: &str,
    review_type: CodeRabbitReviewType,
    base: Option<&str>,
    base_commit: Option<&str>,
) -> Result<CodeRabbitDiffFingerprint, String> {
    let root = workspace_root.trim();
    if root.is_empty() {
        return Err("workspace_root is empty".to_string());
    }

    let head = resolve_current_commit_sha(root)?;
    let current_branch = resolve_current_branch_name(root).ok();
    let base_ref = base_commit
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| value.to_string())
        .or_else(|| resolve_branch_diff_base(root, base));
    let merge_base = base_ref
        .as_deref()
        .and_then(|base_ref| resolve_merge_base(root, base_ref).ok().flatten());

    let staged_diff_hash = match review_type {
        CodeRabbitReviewType::All | CodeRabbitReviewType::Uncommitted => {
            Some(hash_git_output(root, &["diff", "--cached", "--binary"])?)
        }
        CodeRabbitReviewType::Committed => None,
    };
    let unstaged_diff_hash = match review_type {
        CodeRabbitReviewType::All | CodeRabbitReviewType::Uncommitted => {
            Some(hash_git_output(root, &["diff", "--binary"])?)
        }
        CodeRabbitReviewType::Committed => None,
    };
    let untracked_files_hash = match review_type {
        CodeRabbitReviewType::All | CodeRabbitReviewType::Uncommitted => {
            Some(hash_untracked_files(root)?)
        }
        CodeRabbitReviewType::Committed => None,
    };
    let committed_diff_hash = match review_type {
        CodeRabbitReviewType::All | CodeRabbitReviewType::Committed => base_ref
            .as_deref()
            .map(|base_ref| hash_committed_diff(root, base_ref))
            .transpose()?,
        CodeRabbitReviewType::Uncommitted => None,
    };

    let mut combined = Sha256::new();
    combined.update(review_type.as_cli_value().as_bytes());
    combined.update(b"\0");
    update_optional(&mut combined, head.as_deref());
    update_optional(&mut combined, current_branch.as_deref());
    update_optional(&mut combined, base_ref.as_deref());
    update_optional(&mut combined, merge_base.as_deref());
    update_optional(&mut combined, staged_diff_hash.as_deref());
    update_optional(&mut combined, unstaged_diff_hash.as_deref());
    update_optional(&mut combined, untracked_files_hash.as_deref());
    update_optional(&mut combined, committed_diff_hash.as_deref());

    Ok(CodeRabbitDiffFingerprint {
        review_type,
        head,
        current_branch,
        base_ref,
        merge_base,
        staged_diff_hash,
        unstaged_diff_hash,
        untracked_files_hash,
        committed_diff_hash,
        combined_hash: hex_digest(combined.finalize().as_slice()),
        generated_at: Utc::now().to_rfc3339(),
    })
}

fn resolve_merge_base(root: &str, base_ref: &str) -> Result<Option<String>, String> {
    let output = run_git_output(root, &["merge-base", "HEAD", base_ref])?;
    if !output.status.success() {
        return Ok(None);
    }

    let value = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if value.is_empty() {
        Ok(None)
    } else {
        Ok(Some(value))
    }
}

fn hash_committed_diff(root: &str, base_ref: &str) -> Result<String, String> {
    let range = format!("{base_ref}...HEAD");
    hash_git_output(root, &["diff", "--binary", &range])
}

fn hash_git_output(root: &str, args: &[&str]) -> Result<String, String> {
    let output = run_git_output(root, args)?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(if stderr.is_empty() {
            format!("git {} failed", args.join(" "))
        } else {
            stderr
        });
    }

    Ok(hash_bytes(&output.stdout))
}

fn hash_untracked_files(root: &str) -> Result<String, String> {
    let output = run_git_output(root, &["ls-files", "--others", "--exclude-standard", "-z"])?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(if stderr.is_empty() {
            "git ls-files --others failed".to_string()
        } else {
            stderr
        });
    }

    let mut paths = split_null_terminated_fields(&output.stdout);
    paths.sort();
    let mut hasher = Sha256::new();
    for path in paths {
        hasher.update(path.as_bytes());
        hasher.update(b"\0");
        let absolute = PathBuf::from(root).join(&path);
        if absolute.is_file() {
            let bytes = fs::read(&absolute).map_err(|error| error.to_string())?;
            hasher.update(bytes.len().to_string().as_bytes());
            hasher.update(b"\0");
            hasher.update(hash_bytes(&bytes).as_bytes());
        }
        hasher.update(b"\0");
    }

    Ok(hex_digest(hasher.finalize().as_slice()))
}

fn update_optional(hasher: &mut Sha256, value: Option<&str>) {
    if let Some(value) = value {
        hasher.update(value.as_bytes());
    }
    hasher.update(b"\0");
}

fn hash_bytes(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hex_digest(hasher.finalize().as_slice())
}

fn hex_digest(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push_str(&format!("{byte:02x}"));
    }
    out
}
