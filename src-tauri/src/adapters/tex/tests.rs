use super::TexAdapter;
use crate::{
    rewrite_unit::WritebackSlotRole,
    textual_template::{models::TextRegionSplitMode, slots::build_slots},
};

#[test]
fn preserves_text_when_splitting_tex_regions() {
    let text =
        "前文 $E=mc^2$ 后文。\n\\begin{verbatim}\nfn main() {}\n\\end{verbatim}\n% 注释\n末尾";
    let regions = TexAdapter::parse_regions(text, false);
    let rebuilt = regions
        .iter()
        .map(|region| region.body.as_str())
        .collect::<String>();
    assert_eq!(rebuilt, text);
    assert!(regions.iter().any(|region| region.skip_rewrite));
}

#[test]
fn blocks_rewriting_text_inside_heading_commands_by_default() {
    let text = "这是一句。\\section{标题}\n下一句。";
    let regions = TexAdapter::parse_regions(text, false);
    assert!(regions
        .iter()
        .any(|region| region.skip_rewrite && region.body.contains("\\section")));
    assert!(!regions
        .iter()
        .any(|region| !region.skip_rewrite && region.body.contains("标题")));
    let rebuilt = regions
        .iter()
        .map(|region| region.body.as_str())
        .collect::<String>();
    assert_eq!(rebuilt, text);
}

#[test]
fn allows_rewriting_text_inside_heading_commands_when_enabled() {
    let text = "这是一句。\\section{标题}\n下一句。";
    let regions = TexAdapter::parse_regions(text, true);
    assert!(regions
        .iter()
        .any(|region| region.skip_rewrite && region.body.contains("\\section")));
    assert!(regions
        .iter()
        .any(|region| !region.skip_rewrite && region.body.contains("标题")));
    let rebuilt = regions
        .iter()
        .map(|region| region.body.as_str())
        .collect::<String>();
    assert_eq!(rebuilt, text);
}

#[test]
fn allows_rewriting_text_inside_emphasis_commands() {
    let text = "这是 \\textbf{很重要} 的句子。";
    let regions = TexAdapter::parse_regions(text, false);
    assert!(regions
        .iter()
        .any(|region| region.skip_rewrite && region.body.contains("\\textbf{")));
    assert!(regions
        .iter()
        .any(|region| !region.skip_rewrite && region.body.contains("很重要")));
    assert!(regions
        .iter()
        .any(|region| region.skip_rewrite && region.body.contains('}')));
    let rebuilt = regions
        .iter()
        .map(|region| region.body.as_str())
        .collect::<String>();
    assert_eq!(rebuilt, text);
}

#[test]
fn marks_href_as_skip_rewrite() {
    let text = "见 \\href{https://example.com/docs}{https://example.com/docs}。";
    let regions = TexAdapter::parse_regions(text, false);
    assert!(regions
        .iter()
        .any(|region| region.role == WritebackSlotRole::SyntaxToken && region.body.contains("\\href{")));
    assert!(regions.iter().any(|region| {
        region.role == WritebackSlotRole::InlineObject
            && region.body.contains("https://example.com/docs")
    }));
    assert!(regions
        .iter()
        .any(|region| !region.skip_rewrite && region.body.contains("https://example.com/docs")));
    let rebuilt = regions
        .iter()
        .map(|region| region.body.as_str())
        .collect::<String>();
    assert_eq!(rebuilt, text);
}

#[test]
fn marks_texttt_as_skip_rewrite() {
    let text = "命令 \\texttt{cargo fmt --check} 示例。";
    let regions = TexAdapter::parse_regions(text, false);
    assert!(regions
        .iter()
        .any(|region| region.skip_rewrite && region.body.contains("\\texttt{cargo fmt --check}")));
    let rebuilt = regions
        .iter()
        .map(|region| region.body.as_str())
        .collect::<String>();
    assert_eq!(rebuilt, text);
}

#[test]
fn marks_lstinline_as_skip_rewrite() {
    let text = "代码 \\lstinline|fn main() {}| 示例。";
    let regions = TexAdapter::parse_regions(text, false);
    assert!(regions
        .iter()
        .any(|region| region.skip_rewrite && region.body.contains("\\lstinline|fn main() {}|")));
    let rebuilt = regions
        .iter()
        .map(|region| region.body.as_str())
        .collect::<String>();
    assert_eq!(rebuilt, text);
}

#[test]
fn marks_path_as_skip_rewrite() {
    let text = "路径 \\path|C:\\\\a\\\\b| 示例。";
    let regions = TexAdapter::parse_regions(text, false);
    assert!(regions
        .iter()
        .any(|region| region.skip_rewrite && region.body.contains("\\path|C:\\\\a\\\\b|")));
    let rebuilt = regions
        .iter()
        .map(|region| region.body.as_str())
        .collect::<String>();
    assert_eq!(rebuilt, text);
}

#[test]
fn marks_bibliography_environment_as_skip_rewrite() {
    let text = "前文。\n\\begin{thebibliography}{9}\n\\bibitem{a} A.\n\\end{thebibliography}\n后文。";
    let regions = TexAdapter::parse_regions(text, false);
    assert!(regions
        .iter()
        .any(|region| region.skip_rewrite && region.body.contains("\\begin{thebibliography}")));
    let rebuilt = regions
        .iter()
        .map(|region| region.body.as_str())
        .collect::<String>();
    assert_eq!(rebuilt, text);
}

#[test]
fn build_template_keeps_tex_command_shell_locked_and_argument_editable() {
    let template = TexAdapter::build_template("\\textbf{重点}", false);

    assert_eq!(template.kind, "tex");
    assert_eq!(template.blocks.len(), 1);
    assert_eq!(template.blocks[0].anchor, "tex:b0");
    assert_eq!(template.blocks[0].kind, "command_block");
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
                "tex:b0:r0",
                false,
                WritebackSlotRole::SyntaxToken,
                TextRegionSplitMode::Atomic,
            ),
            (
                "tex:b0:r1",
                true,
                WritebackSlotRole::EditableText,
                TextRegionSplitMode::Atomic,
            ),
            (
                "tex:b0:r2",
                false,
                WritebackSlotRole::SyntaxToken,
                TextRegionSplitMode::Atomic,
            ),
        ]
    );
}

#[test]
fn build_template_locks_verbatim_environment_as_single_locked_block() {
    let template = TexAdapter::build_template("\\begin{verbatim}\nraw\n\\end{verbatim}\n", false);

    assert_eq!(template.blocks.len(), 1);
    assert_eq!(template.blocks[0].kind, "locked_block");
    assert!(template.blocks[0].regions.iter().all(|region| !region.editable));
}

#[test]
fn build_template_keeps_href_url_opaque_and_label_atomic() {
    let template = TexAdapter::build_template("\\href{https://example.com}{第一句，第二句}", false);
    let built = build_slots(&template);
    let editable_slots = built
        .slots
        .iter()
        .filter(|slot| slot.editable)
        .collect::<Vec<_>>();

    assert_eq!(editable_slots.len(), 1);
    assert_eq!(editable_slots[0].text, "第一句，第二句");
    assert!(built
        .slots
        .iter()
        .any(|slot| slot.role == WritebackSlotRole::InlineObject && slot.text.contains("https://example.com")));
}

#[test]
fn keeps_multiline_section_argument_in_single_command_block() {
    let template = TexAdapter::build_template("\\section{跨行\n标题}\n正文。", true);
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
    assert_eq!(template.blocks[0].kind, "command_block");
    assert_eq!(block_texts[0], "\\section{跨行\n标题}\n");
    assert_eq!(template.blocks[1].kind, "paragraph");
}
