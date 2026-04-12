#[path = "docx/model.rs"]
mod model;
#[path = "docx/placeholders.rs"]
mod placeholders;
#[path = "docx/simple.rs"]
mod simple;
#[cfg(test)]
#[path = "docx/tests.rs"]
mod tests;

pub use simple::DocxAdapter;
