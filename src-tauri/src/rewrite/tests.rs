use crate::models::{ChunkPreset, DocumentFormat};

use super::*;

#[test]
fn normalize_text_collapses_blank_lines() {
    let input = "第一段\r\n\r\n\r\n第二段\r\n";
    assert_eq!(normalize_text(input), "第一段\n\n第二段");
}

#[test]
fn build_diff_produces_spans() {
    let spans = build_diff("你好", "hollow");
    assert!(!spans.is_empty());
}

#[test]
fn clause_preset_does_not_auto_degrade_by_length() {
    let text = "这是一段没有标点但非常非常非常非常非常非常非常非常非常非常非常非常非常非常非常非常非常非常非常非常非常非常非常非常非常非常非常非常非常非常非常非常非常非常非常非常非常非常非常非常非常非常非常非常非常非常非常非常非常非常非常非常非常非常非常非常非常非常非常非常长的文本";
    let chunks = segment_text(text, ChunkPreset::Clause, DocumentFormat::PlainText, false);

    let rebuilt = chunks
        .iter()
        .map(|chunk| format!("{}{}", chunk.text, chunk.separator_after))
        .collect::<String>();
    assert_eq!(rebuilt, text);

    let editable_chunks: Vec<&SegmentedChunk> = chunks.iter().filter(|c| !c.skip_rewrite).collect();
    assert_eq!(editable_chunks.len(), 1);
    assert_eq!(editable_chunks[0].text, text);
}

#[test]
fn marks_yaml_front_matter_as_skip_rewrite() {
    let text = "---\ntitle: 示例\ntags: [a, b]\n---\n\n正文第一句。";
    let chunks = segment_text(text, ChunkPreset::Sentence, DocumentFormat::Markdown, false);
    assert!(chunks
        .iter()
        .any(|chunk| chunk.skip_rewrite && chunk.text.contains("title: 示例")));

    let rebuilt = chunks
        .iter()
        .map(|chunk| format!("{}{}", chunk.text, chunk.separator_after))
        .collect::<String>();
    assert_eq!(rebuilt, text);
}

#[test]
fn does_not_treat_lonely_horizontal_rule_as_front_matter() {
    // 只有单个 `---` 行且没有闭合 `---`/`...`：更像是 Markdown 水平线，不应把整篇当作 front matter 跳过。
    let text = "---\n\n正文。";
    let chunks = segment_text(text, ChunkPreset::Sentence, DocumentFormat::Markdown, false);
    assert!(chunks.iter().all(|chunk| !chunk.skip_rewrite));

    let rebuilt = chunks
        .iter()
        .map(|chunk| format!("{}{}", chunk.text, chunk.separator_after))
        .collect::<String>();
    assert_eq!(rebuilt, text);
}

#[test]
fn marks_markdown_tables_as_skip_rewrite() {
    let text = "前文。\n\n| a | b |\n|---|---|\n| 1 | 2 |\n\n后文。";
    let chunks = segment_text(text, ChunkPreset::Sentence, DocumentFormat::Markdown, false);
    assert!(chunks.iter().any(|chunk| {
        chunk.skip_rewrite && chunk.text.contains("|---|---|") && chunk.text.contains("| 1 | 2 |")
    }));

    let rebuilt = chunks
        .iter()
        .map(|chunk| format!("{}{}", chunk.text, chunk.separator_after))
        .collect::<String>();
    assert_eq!(rebuilt, text);
}

#[test]
fn marks_markdown_inline_code_as_skip_rewrite() {
    let text = "前文 `let x = 1;` 后文。";
    let chunks = segment_text(text, ChunkPreset::Sentence, DocumentFormat::Markdown, false);
    assert!(chunks
        .iter()
        .any(|chunk| chunk.skip_rewrite && chunk.text.contains("`let x = 1;`")));

    let rebuilt = chunks
        .iter()
        .map(|chunk| format!("{}{}", chunk.text, chunk.separator_after))
        .collect::<String>();
    assert_eq!(rebuilt, text);
}

#[test]
fn marks_markdown_inline_links_as_skip_rewrite() {
    let text = "见 [OpenAI](https://openai.com) 的文档。";
    let chunks = segment_text(text, ChunkPreset::Sentence, DocumentFormat::Markdown, false);
    assert!(chunks.iter().any(|chunk| {
        chunk.skip_rewrite && chunk.text.contains("[OpenAI](https://openai.com)")
    }));

    let rebuilt = chunks
        .iter()
        .map(|chunk| format!("{}{}", chunk.text, chunk.separator_after))
        .collect::<String>();
    assert_eq!(rebuilt, text);
}

#[test]
fn marks_markdown_reference_definitions_as_skip_rewrite() {
    let text = "[id]: https://example.com\n\n正文第一句。";
    let chunks = segment_text(text, ChunkPreset::Sentence, DocumentFormat::Markdown, false);
    assert!(chunks
        .iter()
        .any(|chunk| { chunk.skip_rewrite && chunk.text.contains("[id]: https://example.com") }));

    let rebuilt = chunks
        .iter()
        .map(|chunk| format!("{}{}", chunk.text, chunk.separator_after))
        .collect::<String>();
    assert_eq!(rebuilt, text);
}

#[test]
fn marks_markdown_indented_code_blocks_as_skip_rewrite() {
    let text = "前文。\n\n    fn main() {}\n    println!(\"hi\");\n\n后文。";
    let chunks = segment_text(text, ChunkPreset::Sentence, DocumentFormat::Markdown, false);
    assert!(chunks
        .iter()
        .any(|chunk| chunk.skip_rewrite && chunk.text.contains("fn main()")));

    let rebuilt = chunks
        .iter()
        .map(|chunk| format!("{}{}", chunk.text, chunk.separator_after))
        .collect::<String>();
    assert_eq!(rebuilt, text);
}

#[test]
fn marks_markdown_list_prefix_as_skip_rewrite() {
    let text = "- 第一条\n- 第二条";
    let chunks = segment_text(text, ChunkPreset::Sentence, DocumentFormat::Markdown, false);
    assert!(chunks
        .iter()
        .any(|chunk| chunk.skip_rewrite && chunk.text == "-"));

    let rebuilt = chunks
        .iter()
        .map(|chunk| format!("{}{}", chunk.text, chunk.separator_after))
        .collect::<String>();
    assert_eq!(rebuilt, text);
}

#[test]
fn marks_markdown_emphasis_markers_as_skip_rewrite() {
    let text = "前文 **很重要** 后文。";
    let chunks = segment_text(text, ChunkPreset::Sentence, DocumentFormat::Markdown, false);
    assert!(chunks
        .iter()
        .any(|chunk| chunk.skip_rewrite && chunk.text.contains("**")));
    assert!(chunks
        .iter()
        .any(|chunk| !chunk.skip_rewrite && chunk.text.contains("很重要")));

    let rebuilt = chunks
        .iter()
        .map(|chunk| format!("{}{}", chunk.text, chunk.separator_after))
        .collect::<String>();
    assert_eq!(rebuilt, text);
}

#[test]
fn marks_markdown_footnote_references_as_skip_rewrite() {
    let text = "这是[^1]引用。";
    let chunks = segment_text(text, ChunkPreset::Sentence, DocumentFormat::Markdown, false);
    assert!(chunks
        .iter()
        .any(|chunk| chunk.skip_rewrite && chunk.text.contains("[^1]")));

    let rebuilt = chunks
        .iter()
        .map(|chunk| format!("{}{}", chunk.text, chunk.separator_after))
        .collect::<String>();
    assert_eq!(rebuilt, text);
}

#[test]
fn marks_markdown_footnote_definition_prefix_as_skip_rewrite() {
    let text = "[^1]: 脚注内容。";
    let chunks = segment_text(text, ChunkPreset::Sentence, DocumentFormat::Markdown, false);
    assert!(chunks
        .iter()
        .any(|chunk| chunk.skip_rewrite && chunk.text.contains("[^1]:")));
    assert!(chunks
        .iter()
        .any(|chunk| !chunk.skip_rewrite && chunk.text.contains("脚注内容")));

    let rebuilt = chunks
        .iter()
        .map(|chunk| format!("{}{}", chunk.text, chunk.separator_after))
        .collect::<String>();
    assert_eq!(rebuilt, text);
}

#[test]
fn marks_pandoc_citations_as_skip_rewrite() {
    let text = "如文献[@doe2020]所述。";
    let chunks = segment_text(text, ChunkPreset::Sentence, DocumentFormat::Markdown, false);
    assert!(chunks
        .iter()
        .any(|chunk| chunk.skip_rewrite && chunk.text.contains("[@doe2020]")));

    let rebuilt = chunks
        .iter()
        .map(|chunk| format!("{}{}", chunk.text, chunk.separator_after))
        .collect::<String>();
    assert_eq!(rebuilt, text);
}

#[test]
fn marks_markdown_html_comments_as_skip_rewrite() {
    let text = "前文 <!-- 注释 --> 后文。";
    let chunks = segment_text(text, ChunkPreset::Sentence, DocumentFormat::Markdown, false);
    assert!(chunks
        .iter()
        .any(|chunk| chunk.skip_rewrite && chunk.text.contains("<!-- 注释 -->")));

    let rebuilt = chunks
        .iter()
        .map(|chunk| format!("{}{}", chunk.text, chunk.separator_after))
        .collect::<String>();
    assert_eq!(rebuilt, text);
}

#[test]
fn tex_comment_only_lines_do_not_split_paragraphs() {
    let text = "这是第一句。\n% 这一行只是注释，不应成为段落边界\n这是第二句，仍然同一段。\n";
    let chunks = segment_text(text, ChunkPreset::Paragraph, DocumentFormat::Tex, false);

    let rebuilt = chunks
        .iter()
        .map(|chunk| format!("{}{}", chunk.text, chunk.separator_after))
        .collect::<String>();
    assert_eq!(rebuilt, text);

    let editable_chunks: Vec<&SegmentedChunk> = chunks.iter().filter(|c| !c.skip_rewrite).collect();
    assert_eq!(editable_chunks.len(), 1);
    assert!(editable_chunks[0].text.contains("第一句"));
    assert!(editable_chunks[0].text.contains("第二句"));
}

#[test]
fn detects_stream_required_api_errors() {
    let body = r#"{"error":{"message":"Stream must be set to true","type":"bad_response_status_code","param":"stream"}}"#;
    assert!(super::llm::transport::response_requires_stream(
        reqwest::StatusCode::BAD_REQUEST,
        body
    ));
    assert!(!super::llm::transport::response_requires_stream(
        reqwest::StatusCode::SERVICE_UNAVAILABLE,
        body
    ));
}

#[test]
fn detects_stream_required_api_errors_with_different_wording() {
    let body = r#"{"error":{"message":"This endpoint only supports stream=true"}}"#;
    assert!(super::llm::transport::response_requires_stream(
        reqwest::StatusCode::BAD_REQUEST,
        body
    ));

    let body_zh = r#"{"error":{"message":"stream 必须为 true"}}"#;
    assert!(super::llm::transport::response_requires_stream(
        reqwest::StatusCode::UNPROCESSABLE_ENTITY,
        body_zh
    ));
}

#[test]
fn extracts_compact_api_error_message() {
    let body = r#"{"error":{"message":"Service temporarily unavailable","type":"api_error"}}"#;
    assert_eq!(
        super::llm::transport::extract_api_error_message(body).as_deref(),
        Some("Service temporarily unavailable")
    );
}

#[test]
fn parses_sse_stream_chat_response_body() {
    let body = concat!(
        "data: {\"choices\":[{\"delta\":{\"content\":\"你\"}}]}\n\n",
        "data: {\"choices\":[{\"delta\":{\"content\":\"好\"}}]}\n\n",
        "data: [DONE]\n"
    );
    assert_eq!(
        super::llm::transport::parse_stream_chat_response_body(body).unwrap(),
        "你好".to_string()
    );
}

#[test]
fn parses_ndjson_stream_chat_response_body() {
    let body = concat!(
        "{\"choices\":[{\"delta\":{\"content\":\"你\"}}]}\n",
        "{\"choices\":[{\"delta\":{\"content\":\"好\"}}]}\n"
    );
    assert_eq!(
        super::llm::transport::parse_stream_chat_response_body(body).unwrap(),
        "你好".to_string()
    );
}
