use std::{
    collections::HashMap,
    sync::{atomic::AtomicBool, Arc, Mutex},
};

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
        .map_err(|_| "任务状态锁已损坏。".to_string())?;
    jobs.remove(session_id);
    Ok(())
}
