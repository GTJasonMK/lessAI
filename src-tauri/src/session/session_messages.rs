macro_rules! active_job_pause_cancel_error {
    ($action:literal) => {
        concat!(
            "当前文档正在执行自动任务，请先暂停并取消后再",
            $action,
            "。"
        )
    };
}

pub(crate) const ACTIVE_JOB_RESET_SESSION_ERROR: &str = active_job_pause_cancel_error!("重置");
pub(crate) const ACTIVE_JOB_FINALIZE_ERROR: &str = active_job_pause_cancel_error!("写回原文件");
pub(crate) const ACTIVE_EDITOR_SESSION_ERROR: &str = active_job_pause_cancel_error!("继续编辑");
pub(crate) const ACTIVE_REWRITE_SESSION_ERROR: &str = "当前文档正在执行自动任务，请先暂停或取消。";
