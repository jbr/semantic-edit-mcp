use super::{LanguageCommon, LanguageName};
use crate::languages::traits::LanguageEditor;
use anyhow::Result;
use biome_formatter::IndentWidth;
use biome_js_formatter::{context::JsFormatOptions, format_node};
use biome_js_parser::{parse_module, JsParserOptions};
use biome_js_syntax::JsFileSource;
use biome_rowan::AstNode;

pub fn language() -> Result<LanguageCommon> {
    let language = tree_sitter_javascript::LANGUAGE.into();
    let editor = Box::new(JsEditor);

    Ok(LanguageCommon {
        name: LanguageName::Javascript,
        file_extensions: &["js", "jsx"],
        language,
        editor,
        validation_query: None,
    })
}
struct JsEditor;
impl LanguageEditor for JsEditor {
    fn format_code(&self, source: &str) -> Result<String> {
        // Parse the JavaScript
        let parsed = parse_module(source, JsParserOptions::default());

        // Check for parse errors
        if parsed.has_errors() {
            return Err(anyhow::anyhow!(
                "Parse errors in JavaScript: {:?}",
                parsed.into_diagnostics()
            ));
        }

        // Get the syntax tree
        let syntax_tree = parsed.tree();

        // Format with options
        let options = JsFormatOptions::new(JsFileSource::js_module())
            .with_indent_style(biome_formatter::IndentStyle::Space)
            .with_indent_width(IndentWidth::from(2));
        let formatted = format_node(options, &syntax_tree.into_syntax())?;

        // Convert to string
        let result = formatted.print()?;
        Ok(result.into_code())
    }
}
