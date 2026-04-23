mod blocks;
mod block_support;
mod commands;
mod environments;
mod scan;
mod template;

pub struct TexAdapter;

impl TexAdapter {
    pub fn build_template(
        text: &str,
        rewrite_headings: bool,
    ) -> crate::textual_template::TextTemplate {
        template::build_template(text, rewrite_headings)
    }

    #[cfg(test)]
    pub fn parse_regions(text: &str, rewrite_headings: bool) -> Vec<crate::adapters::TextRegion> {
        commands::parse_regions(text, rewrite_headings)
    }
}

#[cfg(test)]
mod tests;
