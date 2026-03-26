use std::collections::HashSet;

use super::TextRegion;

/// LaTeX/TeX 适配器：识别“语法强约束片段”，并将其标记为 `skip_rewrite`。
///
/// 设计目标：
/// - 让模型只改写自然语言正文，尽量不触碰 TeX 语法（命令/数学/代码环境/注释等）
/// - 输出必须严格保真：regions 拼回去后与原文完全一致
///
/// 说明：
/// - 这里采用保守策略（fail-closed）：宁可多跳过，也不冒险改坏可编译性。
pub struct TexAdapter;

impl TexAdapter {
    pub fn split_regions(text: &str, rewrite_headings: bool) -> Vec<TextRegion> {
        if text.is_empty() {
            return vec![TextRegion {
                body: String::new(),
                skip_rewrite: false,
            }];
        }

        let raw_envs = raw_env_names();
        let math_envs = math_env_names();

        let bytes = text.as_bytes();
        let mut regions: Vec<TextRegion> = Vec::new();

        let mut last = 0usize;
        let mut index = 0usize;

        while index < bytes.len() {
            // 注释：% ... EOL（\% 不算注释）
            if bytes[index] == b'%' && !is_escaped(text, index) {
                push_region(&mut regions, &text[last..index], false);
                let end = find_line_end(text, index);
                push_region(&mut regions, &text[index..end], true);
                index = end;
                last = end;
                continue;
            }

            // 数学模式：$$...$$
            if bytes[index] == b'$'
                && index + 1 < bytes.len()
                && bytes[index + 1] == b'$'
                && !is_escaped(text, index)
            {
                push_region(&mut regions, &text[last..index], false);
                let end = find_closing_double_dollar(text, index + 2).unwrap_or(text.len());
                push_region(&mut regions, &text[index..end], true);
                index = end;
                last = end;
                continue;
            }

            // 数学模式：$...$
            if bytes[index] == b'$'
                && !is_escaped(text, index)
                && !(index + 1 < bytes.len() && bytes[index + 1] == b'$')
            {
                if let Some(end) = find_closing_single_dollar(text, index + 1) {
                    push_region(&mut regions, &text[last..index], false);
                    push_region(&mut regions, &text[index..end], true);
                    index = end;
                    last = end;
                    continue;
                }
            }

            if bytes[index] == b'\\' {
                // 数学模式：\(...\) 与 \[...\]
                // 注意：要避免把 `\\\\[1em]` 这类“换行命令的可选参数”误识别为 `\\[ ... \\]` 数学块。
                // 这里要求开头的反斜杠不能是被前一个反斜杠“转义”的（即不是 `\\\\` 的第二个 `\\`）。
                if text[index..].starts_with("\\(") && !is_escaped(text, index) {
                    if let Some(end) = find_substring(text, index + 2, "\\)") {
                        push_region(&mut regions, &text[last..index], false);
                        push_region(&mut regions, &text[index..end], true);
                        index = end;
                        last = end;
                        continue;
                    }
                }
                if text[index..].starts_with("\\[") && !is_escaped(text, index) {
                    if let Some(end) = find_substring(text, index + 2, "\\]") {
                        push_region(&mut regions, &text[last..index], false);
                        push_region(&mut regions, &text[index..end], true);
                        index = end;
                        last = end;
                        continue;
                    }
                }

                // verbatim / minted / lstlisting 等环境：\begin{...} ... \end{...}
                if let Some((span_start, span_end)) =
                    find_skip_environment_span(text, index, &raw_envs, &math_envs)
                {
                    push_region(&mut regions, &text[last..span_start], false);
                    push_region(&mut regions, &text[span_start..span_end], true);
                    index = span_end;
                    last = span_end;
                    continue;
                }

                // \verb|...| / \verb*|...|
                if let Some(end) = find_inline_verb_end(text, index) {
                    push_region(&mut regions, &text[last..index], false);
                    push_region(&mut regions, &text[index..end], true);
                    index = end;
                    last = end;
                    continue;
                }

                // \lstinline|...| / \lstinline[...|...|（分隔符风格的代码片段）
                if let Some(end) = find_inline_delimited_command_end(text, index, "\\lstinline") {
                    push_region(&mut regions, &text[last..index], false);
                    push_region(&mut regions, &text[index..end], true);
                    index = end;
                    last = end;
                    continue;
                }

                // \path|...|（url 包常用）
                if let Some(end) = find_inline_delimited_command_end(text, index, "\\path") {
                    push_region(&mut regions, &text[last..index], false);
                    push_region(&mut regions, &text[index..end], true);
                    index = end;
                    last = end;
                    continue;
                }

                // 文本型命令：保留命令语法；是否允许改写其参数正文取决于命令类型与设置。
                if let Some((span_end, pieces)) =
                    split_text_command_regions(text, index, rewrite_headings)
                {
                    push_region(&mut regions, &text[last..index], false);
                    for piece in pieces.into_iter() {
                        push_region(&mut regions, &piece.body, piece.skip_rewrite);
                    }
                    index = span_end;
                    last = span_end;
                    continue;
                }

                // 普通命令：\command[...]{...}{...}
                if let Some(end) = find_command_span_end(text, index) {
                    push_region(&mut regions, &text[last..index], false);
                    push_region(&mut regions, &text[index..end], true);
                    index = end;
                    last = end;
                    continue;
                }
            }

            index += 1;
        }

        push_region(&mut regions, &text[last..], false);

        if regions.is_empty() {
            vec![TextRegion {
                body: text.to_string(),
                skip_rewrite: false,
            }]
        } else {
            regions
        }
    }
}

fn push_region(regions: &mut Vec<TextRegion>, body: &str, skip: bool) {
    if body.is_empty() {
        return;
    }
    if let Some(last) = regions.last_mut() {
        if last.skip_rewrite == skip {
            last.body.push_str(body);
            return;
        }
    }
    regions.push(TextRegion {
        body: body.to_string(),
        skip_rewrite: skip,
    });
}

fn raw_env_names() -> HashSet<&'static str> {
    HashSet::from([
        "verbatim",
        "verbatim*",
        "Verbatim",
        "Verbatim*",
        "minted",
        "minted*",
        "lstlisting",
        "lstlisting*",
        "comment",
        "filecontents",
        "filecontents*",
        "tabular",
        "tabular*",
        "longtable",
        "tabu",
        "array",
        "tikzpicture",
        "tikzpicture*",
        "pgfpicture",
        "pgfpicture*",
        "forest",
        "forest*",
        "algorithm",
        "algorithm*",
        "algorithmic",
        "algorithmic*",
        "thebibliography",
        "thebibliography*",
        "bibliography",
        "references",
    ])
}

fn math_env_names() -> HashSet<&'static str> {
    HashSet::from([
        "equation",
        "equation*",
        "align",
        "align*",
        "alignat",
        "alignat*",
        "flalign",
        "flalign*",
        "gather",
        "gather*",
        "multline",
        "multline*",
        "eqnarray",
        "eqnarray*",
        "math",
        "displaymath",
        "split",
        "cases",
        "matrix",
        "pmatrix",
        "bmatrix",
        "vmatrix",
        "Vmatrix",
    ])
}

fn is_escaped(text: &str, index: usize) -> bool {
    if index == 0 {
        return false;
    }
    let bytes = text.as_bytes();
    let mut backslashes = 0usize;
    let mut pos = index;
    while pos > 0 {
        pos -= 1;
        if bytes[pos] == b'\\' {
            backslashes = backslashes.saturating_add(1);
        } else {
            break;
        }
    }
    backslashes % 2 == 1
}

fn find_line_end(text: &str, start: usize) -> usize {
    let bytes = text.as_bytes();
    let mut index = start;
    while index < bytes.len() && bytes[index] != b'\n' && bytes[index] != b'\r' {
        index += 1;
    }
    if index >= bytes.len() {
        return bytes.len();
    }
    if bytes[index] == b'\r' && index + 1 < bytes.len() && bytes[index + 1] == b'\n' {
        index + 2
    } else {
        index + 1
    }
}

fn find_line_start(text: &str, index: usize) -> usize {
    let bytes = text.as_bytes();
    let mut pos = index.min(bytes.len());
    while pos > 0 {
        let prev = pos - 1;
        if bytes[prev] == b'\n' || bytes[prev] == b'\r' {
            break;
        }
        pos -= 1;
    }
    pos
}

fn adjust_to_line_start_if_only_whitespace(text: &str, index: usize, lower_bound: usize) -> usize {
    let line_start = find_line_start(text, index);
    if line_start < lower_bound {
        return index;
    }
    if text[line_start..index].trim().is_empty() {
        line_start
    } else {
        index
    }
}

fn find_substring(text: &str, from: usize, needle: &str) -> Option<usize> {
    text[from..]
        .find(needle)
        .map(|offset| from + offset + needle.len())
}

fn find_closing_double_dollar(text: &str, from: usize) -> Option<usize> {
    let bytes = text.as_bytes();
    let mut index = from;
    while index + 1 < bytes.len() {
        if bytes[index] == b'$' && bytes[index + 1] == b'$' && !is_escaped(text, index) {
            return Some(index + 2);
        }
        index += 1;
    }
    None
}

fn find_closing_single_dollar(text: &str, from: usize) -> Option<usize> {
    let bytes = text.as_bytes();
    let mut index = from;
    while index < bytes.len() {
        if bytes[index] == b'$' && !is_escaped(text, index) {
            return Some(index + 1);
        }
        index += 1;
    }
    None
}

fn find_skip_environment_span(
    text: &str,
    index: usize,
    raw_envs: &HashSet<&'static str>,
    math_envs: &HashSet<&'static str>,
) -> Option<(usize, usize)> {
    if !text[index..].starts_with("\\begin{") {
        return None;
    }

    let name_start = index + "\\begin{".len();
    let name_end = text[name_start..].find('}')? + name_start;
    let env_name = &text[name_start..name_end];
    if env_name.is_empty() {
        return None;
    }

    let is_target = raw_envs.contains(env_name) || math_envs.contains(env_name);
    if !is_target {
        return None;
    }

    let span_start = adjust_to_line_start_if_only_whitespace(text, index, 0);
    let pattern = format!("\\end{{{env_name}}}");
    let search_from = name_end + 1;
    let close_start = text[search_from..]
        .find(&pattern)
        .map(|offset| search_from + offset);
    let span_end = match close_start {
        Some(pos) => {
            let close_end = pos + pattern.len();
            find_line_end(text, close_end)
        }
        None => text.len(),
    };

    Some((span_start, span_end))
}

fn find_inline_verb_end(text: &str, index: usize) -> Option<usize> {
    if !text[index..].starts_with("\\verb") {
        return None;
    }

    let bytes = text.as_bytes();
    let mut pos = index + "\\verb".len();
    if pos < bytes.len() && bytes[pos] == b'*' {
        pos += 1;
    }
    if pos >= bytes.len() {
        return None;
    }

    let delim = bytes[pos] as char;
    if delim.is_whitespace() {
        return None;
    }
    pos += 1;

    while pos < bytes.len() {
        if bytes[pos] as char == delim {
            return Some(pos + 1);
        }
        pos += 1;
    }
    Some(bytes.len())
}

fn find_inline_delimited_command_end(text: &str, index: usize, command: &str) -> Option<usize> {
    if !text[index..].starts_with(command) {
        return None;
    }

    let bytes = text.as_bytes();
    let mut pos = index + command.len();
    if pos < bytes.len() && bytes[pos] == b'*' {
        pos += 1;
    }

    // 可选参数：\lstinline[...]
    loop {
        pos = consume_whitespace(text, pos);
        if pos >= bytes.len() {
            return None;
        }
        if bytes[pos] == b'[' {
            pos = parse_bracket_group(text, pos)?;
            continue;
        }
        break;
    }

    pos = consume_whitespace(text, pos);
    if pos >= bytes.len() {
        return None;
    }

    let delim = bytes[pos];
    if delim.is_ascii_whitespace() || matches!(delim, b'{' | b'}') {
        return None;
    }
    pos += 1;

    while pos < bytes.len() {
        if bytes[pos] == delim {
            return Some(pos + 1);
        }
        pos += 1;
    }

    Some(bytes.len())
}

fn find_command_span_end(text: &str, index: usize) -> Option<usize> {
    if !text[index..].starts_with('\\') {
        return None;
    }
    let bytes = text.as_bytes();
    let mut pos = index + 1;
    if pos >= bytes.len() {
        return Some(index + 1);
    }

    let is_letter = |b: u8| b.is_ascii_alphabetic();
    if is_letter(bytes[pos]) {
        while pos < bytes.len() && is_letter(bytes[pos]) {
            pos += 1;
        }
        if pos < bytes.len() && bytes[pos] == b'*' {
            pos += 1;
        }
    } else {
        // control symbol：\% \{ \\ 等
        pos += 1;
        // 特例：`\\`（换行命令）支持可选 `*` 与可选 `[len]`，应整体锁定为 skip，
        // 否则模型可能会改坏 `[1em]` 这类排版参数，甚至触发误识别 `\\[ ... \\]` 数学块。
        if index + 1 < bytes.len() && bytes[index + 1] == b'\\' {
            if pos < bytes.len() && bytes[pos] == b'*' {
                pos += 1;
            }
            pos = consume_whitespace(text, pos);
            if pos < bytes.len() && bytes[pos] == b'[' {
                pos = parse_bracket_group(text, pos).unwrap_or(bytes.len());
            }
        }
        return Some(pos);
    }

    // 可选/必选参数：尽量保守吞掉，减少语法被模型破坏的可能。
    loop {
        pos = consume_whitespace(text, pos);
        if pos >= bytes.len() {
            break;
        }
        if bytes[pos] == b'[' {
            pos = parse_bracket_group(text, pos).unwrap_or(bytes.len());
            continue;
        }
        if bytes[pos] == b'{' {
            pos = parse_brace_group(text, pos).unwrap_or(bytes.len());
            continue;
        }
        break;
    }

    Some(pos)
}

fn split_text_command_regions(
    text: &str,
    index: usize,
    rewrite_headings: bool,
) -> Option<(usize, Vec<TextRegion>)> {
    let (name, mut pos) = parse_command_name(text, index)?;

    // 只有“字母命令”才有“参数正文”；控制符（例如 \\ \%）不走这里。
    let Some(name) = name else {
        return None;
    };

    let is_heading_command = matches!(
        name,
        "section"
            | "subsection"
            | "subsubsection"
            | "paragraph"
            | "subparagraph"
            | "chapter"
            | "part"
            | "title"
            | "subtitle"
            | "caption"
    );

    // 允许改写其大括号正文的命令白名单（保守挑选：高频强调/注释类）。
    let allow_single_arg = matches!(
        name,
        "footnote" | "emph" | "textbf" | "textit" | "underline" | "textrm" | "textsf" | "textsc"
    );

    // 特例：\href{url}{text} —— 第一个参数是 URL，第二个参数是可读文本。
    let allow_href = name == "href";

    if !is_heading_command && !allow_single_arg && !allow_href {
        return None;
    }

    // 吞掉可选参数与中间空白（全部作为语法的一部分，跳过改写）。
    let bytes = text.as_bytes();
    loop {
        pos = consume_whitespace(text, pos);
        if pos >= bytes.len() {
            return None;
        }
        if bytes[pos] == b'[' {
            pos = parse_bracket_group(text, pos)?;
            continue;
        }
        break;
    }

    if !allow_href {
        if bytes.get(pos) != Some(&b'{') {
            return None;
        }
        let group_end = parse_brace_group(text, pos)?;
        if group_end <= pos + 1 {
            return None;
        }
        let content_start = pos + 1;
        let content_end = group_end - 1;

        if is_heading_command && !rewrite_headings {
            return Some((
                group_end,
                vec![TextRegion {
                    body: text[index..group_end].to_string(),
                    skip_rewrite: true,
                }],
            ));
        }

        let mut out: Vec<TextRegion> = Vec::new();
        out.push(TextRegion {
            body: text[index..content_start].to_string(),
            skip_rewrite: true,
        });

        let inner = TexAdapter::split_regions(&text[content_start..content_end], rewrite_headings);
        out.extend(inner);

        out.push(TextRegion {
            body: text[content_end..group_end].to_string(),
            skip_rewrite: true,
        });

        return Some((group_end, out));
    }

    // \href{url}{text}
    if bytes.get(pos) != Some(&b'{') {
        return None;
    }
    let first_end = parse_brace_group(text, pos)?;

    let mut pos2 = first_end;
    loop {
        pos2 = consume_whitespace(text, pos2);
        if pos2 >= bytes.len() {
            return None;
        }
        if bytes[pos2] == b'[' {
            pos2 = parse_bracket_group(text, pos2)?;
            continue;
        }
        break;
    }
    if bytes.get(pos2) != Some(&b'{') {
        return None;
    }
    let second_end = parse_brace_group(text, pos2)?;
    if second_end <= pos2 + 1 {
        return None;
    }
    let content_start = pos2 + 1;
    let content_end = second_end - 1;

    let mut out: Vec<TextRegion> = Vec::new();
    out.push(TextRegion {
        body: text[index..content_start].to_string(),
        skip_rewrite: true,
    });
    let inner = TexAdapter::split_regions(&text[content_start..content_end], rewrite_headings);
    out.extend(inner);
    out.push(TextRegion {
        body: text[content_end..second_end].to_string(),
        skip_rewrite: true,
    });

    Some((second_end, out))
}

fn parse_command_name(text: &str, index: usize) -> Option<(Option<&str>, usize)> {
    let bytes = text.as_bytes();
    if index >= bytes.len() || bytes[index] != b'\\' {
        return None;
    }
    let mut pos = index + 1;
    if pos >= bytes.len() {
        return None;
    }

    if bytes[pos].is_ascii_alphabetic() {
        let start = pos;
        while pos < bytes.len() && bytes[pos].is_ascii_alphabetic() {
            pos += 1;
        }
        let end = pos;
        // star 变体：\section*{...} 与 \textbf*{...}
        if pos < bytes.len() && bytes[pos] == b'*' {
            pos += 1;
        }
        return Some((Some(&text[start..end]), pos));
    }

    // control symbol：\% \{ \\ 等
    pos += 1;
    Some((None, pos))
}

fn consume_whitespace(text: &str, mut pos: usize) -> usize {
    let bytes = text.as_bytes();
    while pos < bytes.len() && matches!(bytes[pos], b' ' | b'\t' | b'\n' | b'\r') {
        pos += 1;
    }
    pos
}

fn parse_bracket_group(text: &str, start: usize) -> Option<usize> {
    let bytes = text.as_bytes();
    if start >= bytes.len() || bytes[start] != b'[' {
        return None;
    }
    let mut pos = start + 1;
    while pos < bytes.len() {
        match bytes[pos] {
            b'\\' => {
                // 跳过转义，避免把 \] 误判为终止
                pos = (pos + 2).min(bytes.len());
            }
            b']' => return Some(pos + 1),
            _ => pos += 1,
        }
    }
    Some(bytes.len())
}

fn parse_brace_group(text: &str, start: usize) -> Option<usize> {
    let bytes = text.as_bytes();
    if start >= bytes.len() || bytes[start] != b'{' {
        return None;
    }

    let mut depth = 1usize;
    let mut pos = start + 1;
    while pos < bytes.len() {
        match bytes[pos] {
            b'\\' => {
                // 跳过转义，避免把 \{ \} 误算作分组
                pos = (pos + 2).min(bytes.len());
            }
            b'{' => {
                depth = depth.saturating_add(1);
                pos += 1;
            }
            b'}' => {
                depth = depth.saturating_sub(1);
                pos += 1;
                if depth == 0 {
                    return Some(pos);
                }
            }
            _ => pos += 1,
        }
    }
    Some(bytes.len())
}

#[cfg(test)]
mod tests {
    use super::TexAdapter;

    #[test]
    fn preserves_text_when_splitting_tex_regions() {
        let text =
            "前文 $E=mc^2$ 后文。\n\\begin{verbatim}\nfn main() {}\n\\end{verbatim}\n% 注释\n末尾";
        let regions = TexAdapter::split_regions(text, false);
        let rebuilt = regions
            .iter()
            .map(|region| region.body.as_str())
            .collect::<String>();
        assert_eq!(rebuilt, text);
        assert!(regions.iter().any(|r| r.skip_rewrite));
    }

    #[test]
    fn blocks_rewriting_text_inside_heading_commands_by_default() {
        let text = "这是一句。\\section{标题}\n下一句。";
        let regions = TexAdapter::split_regions(text, false);
        assert!(regions
            .iter()
            .any(|r| r.skip_rewrite && r.body.contains("\\section")));
        assert!(!regions
            .iter()
            .any(|r| !r.skip_rewrite && r.body.contains("标题")));
        let rebuilt = regions.iter().map(|r| r.body.as_str()).collect::<String>();
        assert_eq!(rebuilt, text);
    }

    #[test]
    fn allows_rewriting_text_inside_heading_commands_when_enabled() {
        let text = "这是一句。\\section{标题}\n下一句。";
        let regions = TexAdapter::split_regions(text, true);
        assert!(regions
            .iter()
            .any(|r| r.skip_rewrite && r.body.contains("\\section")));
        assert!(regions
            .iter()
            .any(|r| !r.skip_rewrite && r.body.contains("标题")));
        let rebuilt = regions.iter().map(|r| r.body.as_str()).collect::<String>();
        assert_eq!(rebuilt, text);
    }

    #[test]
    fn allows_rewriting_text_inside_emphasis_commands() {
        let text = "这是 \\textbf{很重要} 的句子。";
        let regions = TexAdapter::split_regions(text, false);
        assert!(regions
            .iter()
            .any(|r| r.skip_rewrite && r.body.contains("\\textbf{")));
        assert!(regions
            .iter()
            .any(|r| !r.skip_rewrite && r.body.contains("很重要")));
        assert!(regions
            .iter()
            .any(|r| r.skip_rewrite && r.body.contains('}')));
        let rebuilt = regions.iter().map(|r| r.body.as_str()).collect::<String>();
        assert_eq!(rebuilt, text);
    }

    #[test]
    fn keeps_href_url_as_skip_but_allows_text_argument() {
        let text = "见 \\href{https://example.com}{这里}。";
        let regions = TexAdapter::split_regions(text, false);
        assert!(regions
            .iter()
            .any(|r| r.skip_rewrite && r.body.contains("https://example.com")));
        assert!(regions
            .iter()
            .any(|r| !r.skip_rewrite && r.body.contains("这里")));
        let rebuilt = regions.iter().map(|r| r.body.as_str()).collect::<String>();
        assert_eq!(rebuilt, text);
    }

    #[test]
    fn marks_lstinline_as_skip_rewrite() {
        let text = "代码 \\lstinline|fn main() {}| 示例。";
        let regions = TexAdapter::split_regions(text, false);
        assert!(regions
            .iter()
            .any(|r| { r.skip_rewrite && r.body.contains("\\lstinline|fn main() {}|") }));
        let rebuilt = regions.iter().map(|r| r.body.as_str()).collect::<String>();
        assert_eq!(rebuilt, text);
    }

    #[test]
    fn marks_path_as_skip_rewrite() {
        let text = "路径 \\path|C:\\\\a\\\\b| 示例。";
        let regions = TexAdapter::split_regions(text, false);
        assert!(regions
            .iter()
            .any(|r| r.skip_rewrite && r.body.contains("\\path|C:\\\\a\\\\b|")));
        let rebuilt = regions.iter().map(|r| r.body.as_str()).collect::<String>();
        assert_eq!(rebuilt, text);
    }

    #[test]
    fn marks_bibliography_environment_as_skip_rewrite() {
        let text =
            "前文。\n\\begin{thebibliography}{9}\n\\bibitem{a} A.\n\\end{thebibliography}\n后文。";
        let regions = TexAdapter::split_regions(text, false);
        assert!(regions
            .iter()
            .any(|r| { r.skip_rewrite && r.body.contains("\\begin{thebibliography}") }));
        let rebuilt = regions.iter().map(|r| r.body.as_str()).collect::<String>();
        assert_eq!(rebuilt, text);
    }
}
