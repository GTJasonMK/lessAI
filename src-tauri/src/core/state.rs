use std::{
    collections::HashMap,
    sync::{atomic::AtomicBool, Arc, Mutex},
};

use log::warn;

#[derive(Default)]
pub struct AppState {
    pub(crate) jobs: Mutex<HashMap<String, Arc<JobControl>>>,
    /// 会话文件读写锁（按 session_id 维度）。
    ///
    /// 为什么要有这把锁：
    /// - 会话是 JSON 文件，写入使用 truncate+write；如果 UI 在写入过程中并发读取，会出现 JSON 解析失败。
    /// - 自动批处理引入并发后，会有多个任务同时写入同一个 session 文件，必须串行化。
    pub(crate) session_locks: Mutex<HashMap<String, Arc<Mutex<()>>>>,
}

#[derive(Default)]
pub struct JobControl {
    pub(crate) paused: AtomicBool,
    pub(crate) cancelled: AtomicBool,
}

const JOB_STATE_POISONED_ERROR: &str = "任务状态锁已损坏。";
const ACTIVE_JOB_EXISTS_ERROR: &str = "后台任务仍在运行或正在退出，请稍后再试。";
const DUPLICATE_SESSION_JOB_ERROR: &str = "当前会话已经存在运行中的任务。";

pub(crate) fn session_lock(state: &AppState, session_id: &str) -> Result<Arc<Mutex<()>>, String> {
    let mut locks = state
        .session_locks
        .lock()
        .map_err(|_| "会话锁状态已损坏。".to_string())?;

    Ok(locks
        .entry(session_id.to_string())
        .or_insert_with(|| Arc::new(Mutex::new(())))
        .clone())
}

pub(crate) fn with_session_lock<T>(
    state: &AppState,
    session_id: &str,
    f: impl FnOnce() -> Result<T, String>,
) -> Result<T, String> {
    let lock = session_lock(state, session_id)?;
    let _guard = lock
        .lock()
        .map_err(|_| "会话锁已损坏（可能是上次进程异常退出）。".to_string())?;
    f()
}

pub(crate) fn remove_job(state: &AppState, session_id: &str) -> Result<(), String> {
    let mut jobs = state
        .jobs
        .lock()
        .map_err(|_| JOB_STATE_POISONED_ERROR.to_string())?;
    jobs.remove(session_id);
    Ok(())
}

pub(crate) fn reserve_job(state: &AppState, session_id: &str) -> Result<Arc<JobControl>, String> {
    let mut jobs = state
        .jobs
        .lock()
        .map_err(|_| JOB_STATE_POISONED_ERROR.to_string())?;
    if jobs.contains_key(session_id) {
        return Err(DUPLICATE_SESSION_JOB_ERROR.to_string());
    }

    let job = Arc::new(JobControl::default());
    jobs.insert(session_id.to_string(), job.clone());
    Ok(job)
}

pub(crate) fn ensure_no_active_job(state: &AppState, session_id: &str) -> Result<(), String> {
    let jobs = state
        .jobs
        .lock()
        .map_err(|_| JOB_STATE_POISONED_ERROR.to_string())?;
    if jobs.contains_key(session_id) {
        warn!("rewrite gate blocked: source=live_job_registry session_id={session_id}");
        return Err(ACTIVE_JOB_EXISTS_ERROR.to_string());
    }
    Ok(())
}

pub(crate) fn load_job(
    state: &AppState,
    session_id: &str,
) -> Result<Option<Arc<JobControl>>, String> {
    let jobs = state
        .jobs
        .lock()
        .map_err(|_| JOB_STATE_POISONED_ERROR.to_string())?;
    Ok(jobs.get(session_id).cloned())
}

pub(crate) fn require_job(
    state: &AppState,
    session_id: &str,
    missing_error: &str,
) -> Result<Arc<JobControl>, String> {
    load_job(state, session_id)?.ok_or_else(|| missing_error.to_string())
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::{AppState, JobControl};

    #[test]
    fn ensure_no_active_job_allows_missing_session_job() {
        let state = AppState::default();

        super::ensure_no_active_job(&state, "session-1")
            .expect("expected missing job to be allowed");
    }

    #[test]
    fn ensure_no_active_job_rejects_running_session_job() {
        let state = AppState::default();
        state
            .jobs
            .lock()
            .expect("lock jobs")
            .insert("session-1".to_string(), Arc::new(JobControl::default()));

        let error = super::ensure_no_active_job(&state, "session-1")
            .expect_err("expected running job to be rejected");

        assert_eq!(error, "后台任务仍在运行或正在退出，请稍后再试。");
    }

    #[test]
    fn require_job_rejects_missing_session_job() {
        let state = AppState::default();

        let error = match super::require_job(&state, "session-1", "当前没有可暂停的任务。")
        {
            Ok(_) => panic!("expected missing job to be rejected"),
            Err(error) => error,
        };

        assert_eq!(error, "当前没有可暂停的任务。");
    }

    #[test]
    fn require_job_returns_existing_job() {
        let state = AppState::default();
        let job = Arc::new(JobControl::default());
        state
            .jobs
            .lock()
            .expect("lock jobs")
            .insert("session-1".to_string(), job.clone());

        let loaded = super::require_job(&state, "session-1", "missing")
            .expect("expected existing job to load");

        assert!(Arc::ptr_eq(&job, &loaded));
    }

    #[test]
    fn reserve_job_inserts_and_returns_job_handle() {
        let state = AppState::default();

        let job =
            super::reserve_job(&state, "session-1").expect("expected missing job to be reserved");

        let loaded = super::load_job(&state, "session-1")
            .expect("load job")
            .expect("job should exist after reserve");

        assert!(Arc::ptr_eq(&job, &loaded));
    }

    #[test]
    fn reserve_job_rejects_existing_job() {
        let state = AppState::default();
        state
            .jobs
            .lock()
            .expect("lock jobs")
            .insert("session-1".to_string(), Arc::new(JobControl::default()));

        let error = match super::reserve_job(&state, "session-1") {
            Ok(_) => panic!("expected duplicate reserve to be rejected"),
            Err(error) => error,
        };

        assert_eq!(error, "当前会话已经存在运行中的任务。");
    }
}
