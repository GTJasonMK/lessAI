mod diff;
mod llm;
mod text;

pub use diff::build_diff;
pub use llm::{
    build_client, rewrite_batch, rewrite_batch_with_client, rewrite_selection_text, test_provider,
};
pub use text::{
    convert_line_endings, detect_line_ending, has_trailing_spaces_per_line, normalize_text,
    strip_trailing_spaces_per_line,
};

#[cfg(test)]
mod llm_regression_tests;
#[cfg(test)]
mod tests;
