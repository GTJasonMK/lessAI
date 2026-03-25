#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SegmentedChunk {
    pub text: String,
    /// 该片段后需要拼回去的分隔符（例如空格、换行、段落空行）。
    ///
    /// 设计动机：
    /// - 切块是给 agent/LLM 用的“隐式结构”，不应破坏原文格式；
    /// - 片段之间的空格/换行如果丢失，会导致导出/写回时格式漂移。
    pub separator_after: String,
    /// 是否跳过改写（例如 Markdown fenced code block）。
    ///
    /// 设计动机：
    /// - 代码块/配置片段属于“格式/语义强约束内容”，让模型改写极易改坏；
    /// - 与其提示模型“不要改”，不如直接跳过，保持原样。
    pub skip_rewrite: bool,
}
