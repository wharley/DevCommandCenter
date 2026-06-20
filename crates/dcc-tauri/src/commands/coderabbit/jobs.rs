use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
};

use chrono::Utc;

use super::{
    errors::classify_review_error, parser::ParsedCodeRabbitAgentOutput, CodeRabbitReviewJobStatus,
    WorkspaceCodeRabbitReviewJobSnapshot,
};

#[derive(Clone, Default)]
pub struct CodeRabbitReviewJobsState {
    jobs: Arc<Mutex<HashMap<String, CodeRabbitReviewJobRecord>>>,
}

#[derive(Clone)]
struct CodeRabbitReviewJobRecord {
    snapshot: WorkspaceCodeRabbitReviewJobSnapshot,
    cancel_requested: Arc<AtomicBool>,
}

pub(crate) fn insert_review_job(
    jobs_state: &CodeRabbitReviewJobsState,
    job_id: String,
    snapshot: WorkspaceCodeRabbitReviewJobSnapshot,
    cancel_requested: Arc<AtomicBool>,
) -> Result<(), String> {
    let mut jobs = jobs_state
        .jobs
        .lock()
        .map_err(|_| "CodeRabbit review jobs lock poisoned".to_string())?;
    jobs.insert(
        job_id,
        CodeRabbitReviewJobRecord {
            snapshot,
            cancel_requested,
        },
    );
    Ok(())
}

pub(crate) fn request_review_job_cancel(
    jobs_state: &CodeRabbitReviewJobsState,
    job_id: &str,
) -> Result<Option<u32>, String> {
    let mut jobs = jobs_state
        .jobs
        .lock()
        .map_err(|_| "CodeRabbit review jobs lock poisoned".to_string())?;
    let record = jobs
        .get_mut(job_id)
        .ok_or_else(|| format!("CodeRabbit review job not found: {job_id}"))?;
    record.cancel_requested.store(true, Ordering::Relaxed);
    record.snapshot.cancel_requested = true;
    record.snapshot.updated_at = Utc::now().to_rfc3339();
    record.snapshot.message = Some("Canceling CodeRabbit review".to_string());
    if matches!(
        record.snapshot.status,
        CodeRabbitReviewJobStatus::Starting | CodeRabbitReviewJobStatus::Running
    ) {
        record.snapshot.status = CodeRabbitReviewJobStatus::Running;
    }
    Ok(record.snapshot.pid)
}

pub(crate) fn finish_review_job_canceled(jobs_state: &CodeRabbitReviewJobsState, job_id: &str) {
    update_review_job(jobs_state, job_id, |snapshot| {
        let now = Utc::now().to_rfc3339();
        snapshot.status = CodeRabbitReviewJobStatus::Canceled;
        snapshot.updated_at = now.clone();
        snapshot.completed_at = Some(now);
        snapshot.cancel_requested = true;
        snapshot.message = Some("CodeRabbit review canceled".to_string());
    });
}

pub(crate) fn finish_review_job_error(
    jobs_state: &CodeRabbitReviewJobsState,
    job_id: &str,
    error: String,
) {
    let errors = vec![error];
    let error_kind = classify_review_error(&errors, "", "", false, None);
    update_review_job(jobs_state, job_id, |snapshot| {
        let now = Utc::now().to_rfc3339();
        snapshot.status = CodeRabbitReviewJobStatus::Failed;
        snapshot.updated_at = now.clone();
        snapshot.completed_at = Some(now);
        snapshot.message = Some("CodeRabbit review failed".to_string());
        snapshot.error_kind = error_kind;
        snapshot.errors.extend(errors);
    });
}

pub(crate) fn update_review_job_from_parsed(
    jobs_state: &CodeRabbitReviewJobsState,
    job_id: &str,
    parsed: &ParsedCodeRabbitAgentOutput,
) {
    update_review_job(jobs_state, job_id, |snapshot| {
        snapshot.updated_at = Utc::now().to_rfc3339();
        if let Some(status) = parsed.statuses.last() {
            snapshot.message = status
                .message
                .clone()
                .or_else(|| status.status.clone())
                .or_else(|| Some(status.event_type.clone()));
        }
        if !parsed.errors.is_empty() {
            snapshot.errors = parsed.errors.clone();
        }
    });
}

pub(crate) fn update_review_job(
    jobs_state: &CodeRabbitReviewJobsState,
    job_id: &str,
    update: impl FnOnce(&mut WorkspaceCodeRabbitReviewJobSnapshot),
) {
    if let Ok(mut jobs) = jobs_state.jobs.lock() {
        if let Some(record) = jobs.get_mut(job_id) {
            update(&mut record.snapshot);
        }
    }
}

pub(crate) fn get_review_job_snapshot(
    jobs_state: &CodeRabbitReviewJobsState,
    job_id: &str,
) -> Result<WorkspaceCodeRabbitReviewJobSnapshot, String> {
    let jobs = jobs_state
        .jobs
        .lock()
        .map_err(|_| "CodeRabbit review jobs lock poisoned".to_string())?;
    jobs.get(job_id)
        .map(|record| record.snapshot.clone())
        .ok_or_else(|| format!("CodeRabbit review job not found: {job_id}"))
}
