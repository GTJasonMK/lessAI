mod block_support;
mod blocks;
mod inline_emphasis;
mod inline;
mod inline_lines;
mod inline_scans;
mod inline_spans;
mod syntax;
mod template;

pub struct MarkdownAdapter;

impl MarkdownAdapter {
    pub fn build_template(
        text: &str,
        rewrite_headings: bool,
    ) -> crate::textual_template::TextTemplate {
        template::build_template(text, rewrite_headings)
    }

    #[cfg(test)]
    pub fn parse_block_regions(
        text: &str,
        rewrite_headings: bool,
    ) -> Vec<crate::adapters::TextRegion> {
        blocks::scan_blocks(text)
            .into_iter()
            .flat_map(|block| inline::build_text_regions(&block.text, block.kind, rewrite_headings))
            .collect()
    }
}

#[cfg(test)]
mod tests;
