/// PDF 适配器：从 `.pdf` 中抽取可改写的纯文本（尽量保留“渲染后的换行”）。
///
/// 重要说明：
/// - PDF 不是“文本文件”，很多 PDF 实际上是扫描图片；这种情况无法抽取出正文文本，需要 OCR。
/// - 当前仅支持“导入 → 切块 → 改写 → 导出”为纯文本，不支持写回覆盖 PDF。
/// - 抽取结果的换行来自 PDF 的文本布局启发式（不保证与肉眼看到的换行完全一致）。
pub struct PdfAdapter;

impl PdfAdapter {
    pub fn extract_text(pdf_bytes: &[u8]) -> Result<String, String> {
        if pdf_bytes.is_empty() {
            return Err("pdf 文件为空。".to_string());
        }

        // pdf-extract 会按页输出文本，并通过字符坐标差异启发式插入换行/空格。
        // 我们把页之间用空行隔开，避免把分页直接拼成一行造成阅读困难。
        let pages = pdf_extract::extract_text_from_mem_by_pages(pdf_bytes)
            .map_err(|error| format!("PDF 文本抽取失败：{error}"))?;

        if pages.is_empty() {
            return Err("PDF 文本抽取失败：未解析到任何页面内容。".to_string());
        }

        let mut out = String::new();
        for (index, page) in pages.into_iter().enumerate() {
            if index > 0 {
                out.push_str("\n\n");
            }
            out.push_str(&normalize_extracted_page_text(&page));
        }

        // BOM 在 PDF 提取结果里偶发出现；统一去掉，避免污染首段。
        let cleaned = out.trim_matches('\u{feff}').to_string();

        if cleaned.trim().is_empty() {
            return Err(
                "未从 PDF 中抽取到可见文本。该 PDF 可能是扫描件/图片，需要先做 OCR。".to_string(),
            );
        }

        Ok(cleaned)
    }
}

fn normalize_extracted_page_text(text: &str) -> String {
    // 目标：尽量减少“肉眼不可见但会污染 diff 的噪声”。
    //
    // - 统一换行符为 `\n`
    // - 去掉行尾空格（PDF 抽取常带无意义尾随空格）
    // - 不主动做“行重排为段落”，保持“分行信息”给后续策略使用
    let normalized = text.replace("\r\n", "\n").replace('\r', "\n");
    strip_trailing_spaces_per_line(&normalized)
}

fn strip_trailing_spaces_per_line(text: &str) -> String {
    let mut out = String::with_capacity(text.len());

    for (index, line) in text.split('\n').enumerate() {
        if index > 0 {
            out.push('\n');
        }
        let trimmed = line.trim_end_matches(|ch| ch == ' ' || ch == '\t');
        out.push_str(trimmed);
    }

    out
}
