use crate::adapters::markdown::MarkdownAdapter;
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
fn sentence_preset_does_not_split_on_punct_quoted_as_literal() {
    let text = "整句切分是否在“？”处生效？下一句。";
    let chunks = segment_text(
        text,
        ChunkPreset::Sentence,
        DocumentFormat::PlainText,
        false,
    );
    let editable_chunks: Vec<&SegmentedChunk> = chunks.iter().filter(|c| !c.skip_rewrite).collect();
    assert_eq!(editable_chunks.len(), 2);
    assert_eq!(editable_chunks[0].text, "整句切分是否在“？”处生效？");
    assert_eq!(editable_chunks[1].text, "下一句。");
}

#[test]
fn clause_preset_does_not_split_on_punct_quoted_as_literal() {
    let text = "符号“，”后面紧接文字不应切开。";
    let chunks = segment_text(text, ChunkPreset::Clause, DocumentFormat::PlainText, false);
    let editable_chunks: Vec<&SegmentedChunk> = chunks.iter().filter(|c| !c.skip_rewrite).collect();
    assert_eq!(editable_chunks.len(), 1);
    assert_eq!(editable_chunks[0].text, text);
}

#[test]
fn sentence_preset_does_not_split_on_ascii_abbreviation_dots() {
    let text = "这里包含中英混排（e.g. / i.e. / U.S.A.），以及中文引号“引用内容”。下一句。";
    let chunks = segment_text(
        text,
        ChunkPreset::Sentence,
        DocumentFormat::PlainText,
        false,
    );
    let editable_chunks: Vec<&SegmentedChunk> = chunks.iter().filter(|c| !c.skip_rewrite).collect();
    assert_eq!(editable_chunks.len(), 2);
    assert_eq!(
        editable_chunks[0].text,
        "这里包含中英混排（e.g. / i.e. / U.S.A.），以及中文引号“引用内容”。"
    );
    assert_eq!(editable_chunks[1].text, "下一句。");
}

#[test]
fn sentence_preset_does_not_split_on_filename_dots() {
    let text = "请参考 report.final.v2.pdf。下一句。";
    let chunks = segment_text(
        text,
        ChunkPreset::Sentence,
        DocumentFormat::PlainText,
        false,
    );
    let editable_chunks: Vec<&SegmentedChunk> = chunks.iter().filter(|c| !c.skip_rewrite).collect();
    assert_eq!(editable_chunks.len(), 2);
    assert_eq!(editable_chunks[0].text, "请参考 report.final.v2.pdf。");
    assert_eq!(editable_chunks[1].text, "下一句。");
}

#[test]
fn sentence_preset_does_not_split_on_ascii_ellipsis() {
    let text = "他停顿...然后继续。下一句。";
    let chunks = segment_text(
        text,
        ChunkPreset::Sentence,
        DocumentFormat::PlainText,
        false,
    );
    let editable_chunks: Vec<&SegmentedChunk> = chunks.iter().filter(|c| !c.skip_rewrite).collect();
    assert_eq!(editable_chunks.len(), 2);
    assert_eq!(editable_chunks[0].text, "他停顿...然后继续。");
    assert_eq!(editable_chunks[1].text, "下一句。");
}

#[test]
fn sentence_preset_does_not_split_on_numeric_list_marker_period() {
    let text = "1. 第一条。2. 第二条。";
    let chunks = segment_text(
        text,
        ChunkPreset::Sentence,
        DocumentFormat::PlainText,
        false,
    );
    let editable_chunks: Vec<&SegmentedChunk> = chunks.iter().filter(|c| !c.skip_rewrite).collect();
    assert_eq!(editable_chunks.len(), 2);
    assert_eq!(editable_chunks[0].text, "1. 第一条。");
    assert_eq!(editable_chunks[1].text, "2. 第二条。");
}

#[test]
fn clause_preset_does_not_split_on_url_colon_or_query_mark() {
    let text = "链接 https://example.com/a?b=c&d=e#frag，后文。";
    let chunks = segment_text(text, ChunkPreset::Clause, DocumentFormat::PlainText, false);
    let editable_chunks: Vec<&SegmentedChunk> = chunks.iter().filter(|c| !c.skip_rewrite).collect();
    // Clause 模式会在 `，` 和 `。` 处分割，但 URL 内部不应被 `:` / `?` 切碎。
    assert_eq!(editable_chunks.len(), 2);
    assert_eq!(
        editable_chunks[0].text,
        "链接 https://example.com/a?b=c&d=e#frag，"
    );
    assert_eq!(editable_chunks[1].text, "后文。");
}

#[test]
fn clause_preset_does_not_split_on_windows_drive_colon_or_time_colon() {
    let text = "路径 E:\\\\Code\\\\LessAI\\\\testdoc，时间 12:30，继续。";
    let chunks = segment_text(text, ChunkPreset::Clause, DocumentFormat::PlainText, false);
    let editable_chunks: Vec<&SegmentedChunk> = chunks.iter().filter(|c| !c.skip_rewrite).collect();
    // Clause 模式会在两个 `，` 和最后 `。` 处分割；但 `E:` / `12:30` 不应把块切碎。
    assert_eq!(editable_chunks.len(), 3);
    assert_eq!(
        editable_chunks[0].text,
        "路径 E:\\\\Code\\\\LessAI\\\\testdoc，"
    );
    assert_eq!(editable_chunks[1].text, "时间 12:30，");
    assert_eq!(editable_chunks[2].text, "继续。");
}

#[test]
fn clause_preset_does_not_split_on_fullwidth_comma_in_number() {
    let text = "数值 2，718.28 很常见，后文。";
    let chunks = segment_text(text, ChunkPreset::Clause, DocumentFormat::PlainText, false);
    let editable_chunks: Vec<&SegmentedChunk> = chunks.iter().filter(|c| !c.skip_rewrite).collect();
    assert_eq!(editable_chunks.len(), 2);
    assert_eq!(editable_chunks[0].text, "数值 2，718.28 很常见，");
    assert_eq!(editable_chunks[1].text, "后文。");
}

#[test]
fn clause_preset_does_not_split_on_fullwidth_colon_time() {
    let text = "会议时间 12：30，地点 A。";
    let chunks = segment_text(text, ChunkPreset::Clause, DocumentFormat::PlainText, false);
    let editable_chunks: Vec<&SegmentedChunk> = chunks.iter().filter(|c| !c.skip_rewrite).collect();
    assert_eq!(editable_chunks.len(), 2);
    assert_eq!(editable_chunks[0].text, "会议时间 12：30，");
    assert_eq!(editable_chunks[1].text, "地点 A。");
}

#[test]
fn clause_preset_does_not_split_on_url_port_colon() {
    let text = "链接 https://example.com:8080/path，后文。";
    let chunks = segment_text(text, ChunkPreset::Clause, DocumentFormat::PlainText, false);
    let editable_chunks: Vec<&SegmentedChunk> = chunks.iter().filter(|c| !c.skip_rewrite).collect();
    assert_eq!(editable_chunks.len(), 2);
    assert_eq!(
        editable_chunks[0].text,
        "链接 https://example.com:8080/path，"
    );
    assert_eq!(editable_chunks[1].text, "后文。");
}

#[test]
fn clause_preset_does_not_split_on_url_semicolon() {
    let text = "链接 https://example.com/a;b=c，后文。";
    let chunks = segment_text(text, ChunkPreset::Clause, DocumentFormat::PlainText, false);
    let editable_chunks: Vec<&SegmentedChunk> = chunks.iter().filter(|c| !c.skip_rewrite).collect();
    assert_eq!(editable_chunks.len(), 2);
    assert_eq!(editable_chunks[0].text, "链接 https://example.com/a;b=c，");
    assert_eq!(editable_chunks[1].text, "后文。");
}

#[test]
fn sentence_preset_does_not_split_on_url_hashbang_exclamation() {
    let text = "请看 https://example.com/#!/route。下一句。";
    let chunks = segment_text(
        text,
        ChunkPreset::Sentence,
        DocumentFormat::PlainText,
        false,
    );
    let editable_chunks: Vec<&SegmentedChunk> = chunks.iter().filter(|c| !c.skip_rewrite).collect();
    assert_eq!(editable_chunks.len(), 2);
    assert_eq!(
        editable_chunks[0].text,
        "请看 https://example.com/#!/route。"
    );
    assert_eq!(editable_chunks[1].text, "下一句。");
}

#[test]
fn sentence_preset_keeps_repeated_sentence_punctuation_together() {
    let text = "你每发现“？”将应该分在一起的块给切割开了吗？？下一句。";
    let chunks = segment_text(
        text,
        ChunkPreset::Sentence,
        DocumentFormat::PlainText,
        false,
    );
    let editable_chunks: Vec<&SegmentedChunk> = chunks.iter().filter(|c| !c.skip_rewrite).collect();
    assert_eq!(editable_chunks.len(), 2);
    assert_eq!(
        editable_chunks[0].text,
        "你每发现“？”将应该分在一起的块给切割开了吗？？"
    );
    assert_eq!(editable_chunks[1].text, "下一句。");
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
    // 水平线本身属于纯格式，允许作为 skip chunk；但正文必须可改写。
    assert!(chunks
        .iter()
        .any(|chunk| !chunk.skip_rewrite && chunk.text.contains("正文")));

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
    // 行内结构应在同一 chunk 内显示（不切碎审阅单元），但需要被保护（skip region）。
    let regions = MarkdownAdapter::split_regions(text, false);
    assert!(regions
        .iter()
        .any(|region| region.skip_rewrite && region.body.contains("`let x = 1;`")));
    assert!(chunks
        .iter()
        .any(|chunk| !chunk.skip_rewrite && chunk.text.contains("`let x = 1;`")));

    let rebuilt = chunks
        .iter()
        .map(|chunk| format!("{}{}", chunk.text, chunk.separator_after))
        .collect::<String>();
    assert_eq!(rebuilt, text);
}

#[test]
fn keeps_markdown_inline_code_at_line_end_inside_chunk() {
    let text = "前文 `code`\n下一行继续。";
    let chunks = segment_text(text, ChunkPreset::Sentence, DocumentFormat::Markdown, false);
    assert!(chunks.iter().any(|chunk| chunk.text.contains("`code`")));
    assert!(chunks
        .iter()
        .filter(|chunk| chunk.text.contains("`code`"))
        .all(|chunk| !chunk.skip_rewrite));

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
    let regions = MarkdownAdapter::split_regions(text, false);
    assert!(regions.iter().any(|region| {
        region.skip_rewrite && region.body.contains("[OpenAI](https://openai.com)")
    }));
    assert!(chunks.iter().any(|chunk| {
        !chunk.skip_rewrite && chunk.text.contains("[OpenAI](https://openai.com)")
    }));

    let rebuilt = chunks
        .iter()
        .map(|chunk| format!("{}{}", chunk.text, chunk.separator_after))
        .collect::<String>();
    assert_eq!(rebuilt, text);
}

#[test]
fn keeps_markdown_inline_math_inside_chunk() {
    let text = "当 $x^2 + y^2 = z^2$ 时，称为勾股定理。";
    let chunks = segment_text(text, ChunkPreset::Sentence, DocumentFormat::Markdown, false);
    let regions = MarkdownAdapter::split_regions(text, false);
    assert!(regions
        .iter()
        .any(|region| region.skip_rewrite && region.body.contains("$x^2 + y^2 = z^2$")));
    assert!(chunks
        .iter()
        .any(|chunk| { !chunk.skip_rewrite && chunk.text.contains("$x^2 + y^2 = z^2$") }));

    let rebuilt = chunks
        .iter()
        .map(|chunk| format!("{}{}", chunk.text, chunk.separator_after))
        .collect::<String>();
    assert_eq!(rebuilt, text);
}

#[test]
fn keeps_markdown_inline_math_at_line_end_inside_chunk() {
    let text = "当 $x^2 + y^2 = z^2$\n继续下一行。";
    let chunks = segment_text(text, ChunkPreset::Sentence, DocumentFormat::Markdown, false);
    assert!(chunks
        .iter()
        .any(|chunk| { !chunk.skip_rewrite && chunk.text.contains("$x^2 + y^2 = z^2$") }));

    let rebuilt = chunks
        .iter()
        .map(|chunk| format!("{}{}", chunk.text, chunk.separator_after))
        .collect::<String>();
    assert_eq!(rebuilt, text);
}

#[test]
fn keeps_markdown_inline_math_on_its_own_line_inside_chunk() {
    let text = "前文\n$x+y$\n后文";
    let chunks = segment_text(
        text,
        ChunkPreset::Paragraph,
        DocumentFormat::Markdown,
        false,
    );
    assert!(chunks.iter().any(|chunk| {
        !chunk.skip_rewrite
            && chunk.text.contains("前文")
            && chunk.text.contains("$x+y$")
            && chunk.text.contains("后文")
    }));

    let rebuilt = chunks
        .iter()
        .map(|chunk| format!("{}{}", chunk.text, chunk.separator_after))
        .collect::<String>();
    assert_eq!(rebuilt, text);
}

#[test]
fn markdown_clause_boundary_does_not_split_inside_math() {
    let text = "当 $x, y$ 同时成立时，继续。";
    let chunks = segment_text(text, ChunkPreset::Clause, DocumentFormat::Markdown, false);
    assert!(chunks
        .iter()
        .any(|chunk| { !chunk.skip_rewrite && chunk.text.contains("$x, y$") }));
    // 如果把公式内部的 `,` 当作断句，会出现 `$x,` 这种“被切断”的残片。
    assert!(chunks
        .iter()
        .all(|chunk| { !chunk.text.contains("$x,") || chunk.text.contains("$x, y$") }));

    let rebuilt = chunks
        .iter()
        .map(|chunk| format!("{}{}", chunk.text, chunk.separator_after))
        .collect::<String>();
    assert_eq!(rebuilt, text);
}

#[test]
fn marks_markdown_math_block_as_skip_rewrite() {
    let text = "前文。\n\n$$\nE=mc^2\n$$\n\n后文。";
    let chunks = segment_text(text, ChunkPreset::Sentence, DocumentFormat::Markdown, false);
    assert!(chunks.iter().any(|chunk| {
        chunk.skip_rewrite
            && format!("{}{}", chunk.text, chunk.separator_after).contains("$$\nE=mc^2\n$$")
    }));

    let rebuilt = chunks
        .iter()
        .map(|chunk| format!("{}{}", chunk.text, chunk.separator_after))
        .collect::<String>();
    assert_eq!(rebuilt, text);
}

#[test]
fn keeps_markdown_math_block_inside_paragraph_without_cutting_chunk() {
    let text = "前文。\n$$\nE=mc^2\n$$\n后文。";
    let chunks = segment_text(
        text,
        ChunkPreset::Paragraph,
        DocumentFormat::Markdown,
        false,
    );

    let rebuilt = chunks
        .iter()
        .map(|chunk| format!("{}{}", chunk.text, chunk.separator_after))
        .collect::<String>();
    assert_eq!(rebuilt, text);

    let editable_chunks: Vec<&SegmentedChunk> = chunks.iter().filter(|c| !c.skip_rewrite).collect();
    assert_eq!(editable_chunks.len(), 1);
    assert!(editable_chunks[0].text.contains("前文"));
    assert!(editable_chunks[0].text.contains("$$"));
    assert!(editable_chunks[0].text.contains("E=mc^2"));
    assert!(editable_chunks[0].text.contains("后文"));
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
    // 列表符号属于保护区，但不应独立成为审阅单元。
    assert!(!chunks.iter().any(|chunk| chunk.text == "-"));
    assert!(chunks.iter().any(|chunk| chunk.text.contains("- 第一条")));
    assert!(chunks.iter().any(|chunk| chunk.text.contains("- 第二条")));

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
    let regions = MarkdownAdapter::split_regions(text, false);
    assert!(regions
        .iter()
        .any(|region| region.skip_rewrite && region.body.contains("**")));
    assert!(chunks
        .iter()
        .any(|chunk| !chunk.skip_rewrite && chunk.text.contains("**很重要**")));

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
    let regions = MarkdownAdapter::split_regions(text, false);
    assert!(regions
        .iter()
        .any(|region| region.skip_rewrite && region.body.contains("[^1]")));
    assert!(chunks
        .iter()
        .any(|chunk| !chunk.skip_rewrite && chunk.text.contains("[^1]")));

    let rebuilt = chunks
        .iter()
        .map(|chunk| format!("{}{}", chunk.text, chunk.separator_after))
        .collect::<String>();
    assert_eq!(rebuilt, text);
}

#[test]
fn marks_markdown_footnote_definition_as_skip_rewrite() {
    let text = "[^1]: 脚注内容。";
    let chunks = segment_text(text, ChunkPreset::Sentence, DocumentFormat::Markdown, false);
    assert!(chunks.iter().any(|chunk| chunk.skip_rewrite
        && chunk.text.contains("[^1]:")
        && chunk.text.contains("脚注内容")));

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
    let regions = MarkdownAdapter::split_regions(text, false);
    assert!(regions
        .iter()
        .any(|region| region.skip_rewrite && region.body.contains("[@doe2020]")));
    assert!(chunks
        .iter()
        .any(|chunk| !chunk.skip_rewrite && chunk.text.contains("[@doe2020]")));

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
    let regions = MarkdownAdapter::split_regions(text, false);
    assert!(regions
        .iter()
        .any(|region| region.skip_rewrite && region.body.contains("<!-- 注释 -->")));
    assert!(chunks
        .iter()
        .any(|chunk| !chunk.skip_rewrite && chunk.text.contains("<!-- 注释 -->")));

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
fn tex_display_math_blocks_do_not_cut_paragraph_chunks() {
    let text = "这是前文。\n\\[\nE=mc^2\n\\]\n这是后文，仍然同一段。\n";
    let chunks = segment_text(text, ChunkPreset::Paragraph, DocumentFormat::Tex, false);

    let rebuilt = chunks
        .iter()
        .map(|chunk| format!("{}{}", chunk.text, chunk.separator_after))
        .collect::<String>();
    assert_eq!(rebuilt, text);

    let editable_chunks: Vec<&SegmentedChunk> = chunks.iter().filter(|c| !c.skip_rewrite).collect();
    assert_eq!(editable_chunks.len(), 1);
    assert!(editable_chunks[0].text.contains("前文"));
    assert!(editable_chunks[0].text.contains("\\["));
    assert!(editable_chunks[0].text.contains("E=mc^2"));
    assert!(editable_chunks[0].text.contains("后文"));
}

#[test]
fn tex_math_environments_do_not_cut_paragraph_chunks() {
    let text = "前文。\n\\begin{align}\na &= b \\\\\nc &= d\n\\end{align}\n后文。\n";
    let chunks = segment_text(text, ChunkPreset::Paragraph, DocumentFormat::Tex, false);

    let rebuilt = chunks
        .iter()
        .map(|chunk| format!("{}{}", chunk.text, chunk.separator_after))
        .collect::<String>();
    assert_eq!(rebuilt, text);

    let editable_chunks: Vec<&SegmentedChunk> = chunks.iter().filter(|c| !c.skip_rewrite).collect();
    assert_eq!(editable_chunks.len(), 1);
    assert!(editable_chunks[0].text.contains("前文"));
    assert!(editable_chunks[0].text.contains("\\begin{align}"));
    assert!(editable_chunks[0].text.contains("\\end{align}"));
    assert!(editable_chunks[0].text.contains("后文"));
}

#[test]
fn tex_double_dollar_math_blocks_do_not_cut_paragraph_chunks() {
    let text = "这是前文。\n$$\nE=mc^2\n$$\n这是后文，仍然同一段。\n";
    let chunks = segment_text(text, ChunkPreset::Paragraph, DocumentFormat::Tex, false);

    let rebuilt = chunks
        .iter()
        .map(|chunk| format!("{}{}", chunk.text, chunk.separator_after))
        .collect::<String>();
    assert_eq!(rebuilt, text);

    let editable_chunks: Vec<&SegmentedChunk> = chunks.iter().filter(|c| !c.skip_rewrite).collect();
    assert_eq!(editable_chunks.len(), 1);
    assert!(editable_chunks[0].text.contains("前文"));
    assert!(editable_chunks[0].text.contains("$$"));
    assert!(editable_chunks[0].text.contains("E=mc^2"));
    assert!(editable_chunks[0].text.contains("后文"));
}

#[test]
fn tex_linebreak_optional_arg_is_not_math_block() {
    let text = "第一行\\\\[1em]\n第二行仍然是正文。\n";
    let chunks = segment_text(text, ChunkPreset::Paragraph, DocumentFormat::Tex, false);

    let rebuilt = chunks
        .iter()
        .map(|chunk| format!("{}{}", chunk.text, chunk.separator_after))
        .collect::<String>();
    assert_eq!(rebuilt, text);

    let editable_chunks: Vec<&SegmentedChunk> = chunks.iter().filter(|c| !c.skip_rewrite).collect();
    assert_eq!(editable_chunks.len(), 1);
    assert!(editable_chunks[0].text.contains("\\\\[1em]"));
    assert!(editable_chunks[0].text.contains("第二行仍然是正文"));
}

#[test]
fn tex_sentence_preset_does_not_split_on_items_without_blank_lines() {
    // 层级语义：分句/整句应当发生在“段落块”内部；
    // 如果没有空行/\\par，Sentence 预设不应因为 `\\item` 这种结构命令而强行切块。
    let text = concat!(
        "\\begin{enumerate}\n",
        "\\item 第一项内容\n",
        "\\item 第二项内容\n",
        "\\end{enumerate}\n",
    );
    let chunks = segment_text(text, ChunkPreset::Sentence, DocumentFormat::Tex, false);

    let rebuilt = chunks
        .iter()
        .map(|chunk| format!("{}{}", chunk.text, chunk.separator_after))
        .collect::<String>();
    assert_eq!(rebuilt, text);

    let editable_chunks: Vec<&SegmentedChunk> = chunks.iter().filter(|c| !c.skip_rewrite).collect();
    assert_eq!(editable_chunks.len(), 1);
    assert!(editable_chunks[0].text.contains("\\item 第一项内容"));
    assert!(editable_chunks[0].text.contains("\\item 第二项内容"));
}

#[test]
fn tex_blank_line_splits_paragraph_chunks() {
    // TeX/LaTeX 的“段落”由空行（或 `\par`）触发：
    // - 单个换行通常在渲染中视为段内空格；
    // - 空行（两次换行，即出现一行空内容）才是段落边界。
    let text = "第一句。\n\n第二句仍然属于同一块。\n";
    let chunks = segment_text(text, ChunkPreset::Paragraph, DocumentFormat::Tex, false);

    let rebuilt = chunks
        .iter()
        .map(|chunk| format!("{}{}", chunk.text, chunk.separator_after))
        .collect::<String>();
    assert_eq!(rebuilt, text);

    let editable_chunks: Vec<&SegmentedChunk> = chunks.iter().filter(|c| !c.skip_rewrite).collect();
    assert_eq!(editable_chunks.len(), 2);
    assert!(editable_chunks[0].text.contains("第一句"));
    assert!(editable_chunks[1].text.contains("第二句"));
}

#[test]
fn tex_double_blank_lines_split_paragraph_chunks() {
    // 连续两个空行更像“视觉分段证据”，此时应产生块边界。
    let text = "第一句。\n\n\n第二句。\n";
    let chunks = segment_text(text, ChunkPreset::Paragraph, DocumentFormat::Tex, false);

    let rebuilt = chunks
        .iter()
        .map(|chunk| format!("{}{}", chunk.text, chunk.separator_after))
        .collect::<String>();
    assert_eq!(rebuilt, text);

    let editable_chunks: Vec<&SegmentedChunk> = chunks.iter().filter(|c| !c.skip_rewrite).collect();
    assert_eq!(editable_chunks.len(), 2);
    assert!(editable_chunks[0].text.contains("第一句"));
    assert!(editable_chunks[1].text.contains("第二句"));
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
