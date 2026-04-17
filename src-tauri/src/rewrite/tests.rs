use crate::models::DiffType;

use super::{
    build_diff, convert_line_endings, detect_line_ending, normalize_text,
    strip_trailing_spaces_per_line,
};

#[test]
fn normalize_text_collapses_blank_lines_and_trims_each_line() {
    let input = " 第一段 \r\n\r\n\r\n 第二段\t\r\n";

    assert_eq!(normalize_text(input), "第一段\n\n第二段");
}

#[test]
fn build_diff_reconstructs_candidate_text() {
    fn rebuild_candidate(spans: &[crate::models::DiffSpan]) -> String {
        spans
            .iter()
            .filter(|span| span.r#type != DiffType::Delete)
            .map(|span| span.text.as_str())
            .collect::<String>()
    }

    let cases = [
        ("", ""),
        ("a", "a"),
        ("a", ""),
        ("", "a"),
        ("abc", "abXc"),
        ("你好", "你好呀"),
        ("Hello\nWorld\n", "Hello\nNew World\n"),
        ("1. 第一条。\n2. 第二条。\n", "1. 第一条。\n2. 修改。\n"),
    ];

    for (before, after) in cases {
        let spans = build_diff(before, after);
        assert_eq!(rebuild_candidate(&spans), after);
    }
}

#[test]
fn detect_and_convert_line_endings_preserve_dominant_style() {
    let text = "第一行\r\n第二行\r\n第三行\n";

    assert_eq!(detect_line_ending(text), super::text::LineEnding::CrLf);
    assert_eq!(
        convert_line_endings("甲\n乙\n", super::text::LineEnding::CrLf),
        "甲\r\n乙\r\n"
    );
}

#[test]
fn strip_trailing_spaces_removes_only_line_tail_padding() {
    let input = "甲  \r\n乙\t\t\n丙";

    assert_eq!(strip_trailing_spaces_per_line(input), "甲\r\n乙\n丙");
}
