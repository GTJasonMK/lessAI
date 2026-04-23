use super::MarkdownAdapter;
use crate::{
    rewrite_unit::WritebackSlotRole,
    textual_template::{models::TextRegionSplitMode, slots::build_slots},
};

#[test]
fn build_template_marks_markdown_syntax_shells_as_locked_regions() {
    let template = MarkdownAdapter::build_template("1. [标题](https://example.com)\n", false);

    assert_eq!(template.kind, "markdown");
    assert_eq!(template.blocks.len(), 1);
    assert_eq!(template.blocks[0].anchor, "md:b0");
    assert_eq!(template.blocks[0].kind, "list_item");
    assert_eq!(
        template.blocks[0]
            .regions
            .iter()
            .map(|region| {
                (
                    region.anchor.as_str(),
                    region.editable,
                    region.role.clone(),
                    region.split_mode,
                )
            })
            .collect::<Vec<_>>(),
        vec![
            (
                "md:b0:r0",
                false,
                WritebackSlotRole::SyntaxToken,
                TextRegionSplitMode::Atomic,
            ),
            (
                "md:b0:r1",
                false,
                WritebackSlotRole::SyntaxToken,
                TextRegionSplitMode::Atomic,
            ),
            (
                "md:b0:r2",
                true,
                WritebackSlotRole::EditableText,
                TextRegionSplitMode::Atomic,
            ),
            (
                "md:b0:r3",
                false,
                WritebackSlotRole::SyntaxToken,
                TextRegionSplitMode::Atomic,
            ),
            (
                "md:b0:r4",
                false,
                WritebackSlotRole::InlineObject,
                TextRegionSplitMode::Atomic,
            ),
            (
                "md:b0:r5",
                false,
                WritebackSlotRole::SyntaxToken,
                TextRegionSplitMode::Atomic,
            ),
        ]
    );
}

#[test]
fn build_template_locks_fenced_code_block_as_single_locked_block() {
    let template = MarkdownAdapter::build_template("```rust\nfn main() {}\n```\n", false);

    assert_eq!(template.blocks.len(), 1);
    assert_eq!(template.blocks[0].kind, "locked_block");
    assert!(template.blocks[0].regions.iter().all(|region| !region.editable));
}

#[test]
fn preserves_text_when_splitting_markdown_regions() {
    let text = "---\ntitle: 测试\n---\n\n# 标题\n\n这里是正文，包含 `inline code` 和 [链接](https://example.com)。\n\n```rust\nfn main() {}\n```\n\n|a|b|\n|---|---|\n|1|2|\n";
    let regions = MarkdownAdapter::parse_block_regions(text, false);
    let rebuilt = regions
        .iter()
        .map(|region| region.body.as_str())
        .collect::<String>();
    assert_eq!(rebuilt, text);
    assert!(regions.iter().any(|r| r.skip_rewrite));
}

#[test]
fn protects_inline_html_tags_and_single_emphasis_markers() {
    let text = "按 <kbd>Ctrl</kbd> + <kbd>S</kbd> 保存，这是 *重点* 和 _斜体_。\n下一行。";
    let regions = MarkdownAdapter::parse_block_regions(text, false);
    let rebuilt = regions
        .iter()
        .map(|region| region.body.as_str())
        .collect::<String>();
    assert_eq!(rebuilt, text);

    assert!(regions
        .iter()
        .any(|r| r.skip_rewrite && r.body.contains("<kbd")));
    assert!(regions.iter().any(|r| r.skip_rewrite && r.body == "*"));
    assert!(regions
        .iter()
        .any(|r| !r.skip_rewrite && r.body.contains("重点")));
    assert!(regions.iter().any(|r| r.skip_rewrite && r.body == "_"));
    assert!(regions
        .iter()
        .any(|r| !r.skip_rewrite && r.body.contains("斜体")));
}

#[test]
fn does_not_treat_intraword_underscore_as_emphasis() {
    let text = "foo_bar_baz";
    let regions = MarkdownAdapter::parse_block_regions(text, false);
    let rebuilt = regions
        .iter()
        .map(|region| region.body.as_str())
        .collect::<String>();
    assert_eq!(rebuilt, text);
    assert!(!regions.iter().any(|r| r.skip_rewrite && r.body == "_"));
}

#[test]
fn preserves_nested_emphasis_delimiters_by_rule() {
    let text = "***重点*** 和 **粗体 _斜体_**";
    let regions = MarkdownAdapter::parse_block_regions(text, false);
    let rebuilt = regions
        .iter()
        .map(|region| region.body.as_str())
        .collect::<String>();
    let locked_delimiters = regions
        .iter()
        .filter(|region| region.skip_rewrite)
        .flat_map(|region| region.body.chars())
        .filter(|ch| matches!(ch, '*' | '_' | '~'))
        .collect::<String>();
    let expected_delimiters = text
        .chars()
        .filter(|ch| matches!(ch, '*' | '_' | '~'))
        .collect::<String>();

    assert_eq!(rebuilt, text);
    assert_eq!(locked_delimiters, expected_delimiters);
    assert!(regions
        .iter()
        .any(|region| !region.skip_rewrite && region.body.contains("重点")));
    assert!(regions
        .iter()
        .any(|region| !region.skip_rewrite && region.body.contains("粗体")));
    assert!(regions
        .iter()
        .any(|region| !region.skip_rewrite && region.body.contains("斜体")));
}

#[test]
fn leaves_unmatched_or_space_padded_markers_editable() {
    let text = "普通文本 * foo * 与 **未闭合";
    let regions = MarkdownAdapter::parse_block_regions(text, false);
    let rebuilt = regions
        .iter()
        .map(|region| region.body.as_str())
        .collect::<String>();
    let locked_delimiters = regions
        .iter()
        .filter(|region| region.skip_rewrite)
        .flat_map(|region| region.body.chars())
        .filter(|ch| matches!(ch, '*' | '_' | '~'))
        .collect::<String>();

    assert_eq!(rebuilt, text);
    assert!(locked_delimiters.is_empty());
}

#[test]
fn keeps_link_label_atomic_when_building_slots() {
    let template = MarkdownAdapter::build_template("[第一句，第二句](https://example.com)\n", false);
    let built = build_slots(&template);
    let editable_slots = built
        .slots
        .iter()
        .filter(|slot| slot.editable)
        .collect::<Vec<_>>();

    assert_eq!(editable_slots.len(), 1);
    assert_eq!(editable_slots[0].text, "第一句，第二句");
}

#[test]
fn keeps_nested_quote_inside_single_list_item_block() {
    let template = MarkdownAdapter::build_template("- 第一项\n  > 引用内容\n- 第二项\n", false);
    let block_texts = template
        .blocks
        .iter()
        .map(|block| {
            block
                .regions
                .iter()
                .map(|region| format!("{}{}", region.text, region.separator_after))
                .collect::<String>()
        })
        .collect::<Vec<_>>();

    assert_eq!(template.blocks.len(), 2);
    assert_eq!(block_texts[0], "- 第一项\n  > 引用内容\n");
    assert_eq!(block_texts[1], "- 第二项\n");
}
