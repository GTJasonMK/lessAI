#[path = "docx/display.rs"]
mod display;
#[path = "docx/model.rs"]
mod model;
#[path = "docx/numbering.rs"]
mod numbering;
#[path = "docx/package.rs"]
mod package;
#[cfg(test)]
#[path = "docx/package_tests.rs"]
mod package_tests;
#[path = "docx/placeholders.rs"]
mod placeholders;
#[path = "docx/simple.rs"]
mod simple;
#[path = "docx/slots.rs"]
mod slots;
#[path = "docx/specials.rs"]
mod specials;
#[path = "docx/styles.rs"]
mod styles;
#[cfg(test)]
#[path = "docx/tests.rs"]
mod tests;
#[cfg(test)]
#[path = "docx/tests_hardcoding.rs"]
mod tests_hardcoding;
#[path = "docx/xml.rs"]
mod xml;
#[cfg(test)]
#[path = "docx/xml_tests.rs"]
mod xml_tests;

pub use simple::DocxAdapter;
