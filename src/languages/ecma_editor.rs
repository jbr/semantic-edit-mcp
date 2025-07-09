use std::path::Path;

use crate::languages::{common::Indentation, traits::LanguageEditor};
use anyhow::Result;
use biome_console::fmt::Termcolor;
use biome_console::markup;
use biome_diagnostics::{console::fmt::Formatter, termcolor::Buffer, PrintDiagnostic};
use biome_formatter::{IndentStyle, IndentWidth};
use biome_js_formatter::{context::JsFormatOptions, format_node};
use biome_js_parser::JsParserOptions;
use biome_js_syntax::JsFileSource;
use biome_parser::prelude::ParseDiagnostic;
use biome_rowan::AstNode;

pub(super) enum EcmaEditor {
    Js,
    Ts,
    Tsx,
    Jsx,
}
impl LanguageEditor for EcmaEditor {
    fn format_code(&self, source: &str, file_path: &Path) -> Result<String> {
        let source_file = file_path.try_into().unwrap_or(JsFileSource::js_module());

        let parsed = biome_js_parser::parse(source, source_file, JsParserOptions::default());

        // Check for parse errors
        if parsed.has_errors() {
            return Err(anyhow::anyhow!(
                "Parse errors: {}",
                format_diagnostics(parsed.diagnostics())
            ));
        }

        // Get the syntax tree
        let syntax_tree = parsed.tree();

        // Format with options
        let mut options = JsFormatOptions::new(source_file);

        match Indentation::determine(source).unwrap_or(Indentation::Spaces(2)) {
            Indentation::Spaces(spaces) => {
                options.set_indent_style(IndentStyle::Space);
                options.set_indent_width(IndentWidth::from(spaces));
            }

            Indentation::Tabs => {
                options.set_indent_style(IndentStyle::Tab);
            }
        }

        let formatted = format_node(options, &syntax_tree.into_syntax())?;

        // Convert to string
        let result = formatted.print()?;
        Ok(result.into_code())
    }
}

fn format_diagnostics(diagnostics: &[ParseDiagnostic]) -> String {
    let mut write = Buffer::no_color();
    let mut termcolor = Termcolor(&mut write);
    let mut formatter = Formatter::new(&mut termcolor);
    for diagnostic in diagnostics {
        formatter
            .write_markup(markup! {
                {PrintDiagnostic::simple(diagnostic)}
            })
            .unwrap();
    }

    String::from_utf8_lossy(write.as_slice()).into_owned()
}
