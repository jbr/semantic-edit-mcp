//! Formatting: which formatter, if any, reformats a file after an edit.
//!
//! Formatters are configuration, separate from languages: [`Formatting`] maps
//! each [`LanguageName`] to a [`Formatter`] or to none, and a [`Formatter`]
//! carries its own invocation. An edit to a file whose language has no formatter
//! is written as laid out.

mod command;
mod rustfmt;

use crate::{indentation::Indentation, languages::LanguageName};
use anyhow::Result;
use enum_map::{EnumMap, enum_map};
use std::path::Path;

/// The formatter, if any, for each language's files.
///
/// A formatter reformats the whole file after each edit to it, and an edit it
/// fails on is rejected. A language without one has its edits written as laid
/// out.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Formatting {
    formatters: EnumMap<LanguageName, Option<Formatter>>,
}

impl Default for Formatting {
    /// rustfmt for Rust, ruff for Python, biome for JavaScript, JSX, TypeScript,
    /// TSX and JSON, and taplo for TOML. Other files have no formatter.
    fn default() -> Self {
        Self {
            formatters: enum_map! {
                LanguageName::Rust => Some(Formatter::rustfmt()),
                LanguageName::Python => Some(Formatter::ruff()),
                LanguageName::Javascript
                | LanguageName::Jsx
                | LanguageName::Typescript
                | LanguageName::Tsx
                | LanguageName::Json => Some(Formatter::biome()),
                LanguageName::Toml => Some(Formatter::taplo()),
                LanguageName::Other => None,
            },
        }
    }
}

impl Formatting {
    /// No formatter for any language: every edit is written as laid out.
    #[allow(dead_code, reason = "embedding API, used by library consumers")]
    pub fn off() -> Self {
        Self {
            formatters: EnumMap::default(),
        }
    }

    /// These settings, with `language` formatted by `formatter`.
    #[allow(dead_code, reason = "embedding API, used by library consumers")]
    pub fn with(mut self, language: LanguageName, formatter: Formatter) -> Self {
        self.set(language, Some(formatter));
        self
    }

    /// These settings, with no formatter for `language`.
    #[allow(dead_code, reason = "embedding API, used by library consumers")]
    pub fn without(mut self, language: LanguageName) -> Self {
        self.set(language, None);
        self
    }

    /// Format `language` with `formatter`, or with none.
    #[allow(dead_code, reason = "embedding API, used by library consumers")]
    pub fn set(&mut self, language: LanguageName, formatter: Option<Formatter>) -> &mut Self {
        self.formatters[language] = formatter;
        self
    }

    /// The formatter for `language`'s files, if it has one.
    pub fn formatter(&self, language: LanguageName) -> Option<&Formatter> {
        self.formatters[language].as_ref()
    }
}

/// A formatter, which reformats a file's whole source.
///
/// Each constructor carries its tool's invocation. The program-running ones
/// read the source on stdin and write the result to stdout, and must be
/// installed where the edit runs; a missing one fails the edit.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Formatter {
    kind: Kind,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    Rustfmt,
    Ruff,
    Biome,
    Taplo,
}

impl Formatter {
    /// rustfmt, for Rust, at the edition the nearest `Cargo.toml` declares, or
    /// 2024 when none does.
    pub fn rustfmt() -> Self {
        Self {
            kind: Kind::Rustfmt,
        }
    }

    /// `ruff format`, for Python.
    pub fn ruff() -> Self {
        Self { kind: Kind::Ruff }
    }

    /// `biome format`, for JavaScript, JSX, TypeScript, TSX and JSON, given the
    /// file's path, and set to the indentation the file already uses.
    pub fn biome() -> Self {
        Self { kind: Kind::Biome }
    }

    /// taplo, for TOML, with its default options. It runs in-process, so
    /// nothing needs to be installed.
    pub fn taplo() -> Self {
        Self { kind: Kind::Taplo }
    }

    /// `source` formatted as the content of the file at `path`, or the
    /// formatter's error.
    pub(crate) fn format(&self, source: &str, path: &Path) -> Result<String> {
        match self.kind {
            Kind::Rustfmt => command::run(
                "rustfmt",
                ["--emit", "stdout", "--edition", rustfmt::edition(path)],
                source,
            ),
            Kind::Ruff => command::run("ruff", ["format", "-"], source),
            Kind::Biome => command::run("biome", biome_args(source, path), source),
            Kind::Taplo => Ok(taplo::formatter::format(
                source,
                taplo::formatter::Options::default(),
            )),
        }
    }
}

fn biome_args(source: &str, path: &Path) -> Vec<String> {
    let mut args = vec![
        String::from("format"),
        format!("--stdin-file-path={}", path.display()),
        String::from("--diagnostic-level=error"),
    ];
    match Indentation::determine(source).unwrap_or(Indentation::Spaces(2)) {
        Indentation::Spaces(spaces) => {
            args.push(String::from("--indent-style=space"));
            args.push(format!("--indent-width={spaces}"));
        }
        Indentation::Tabs => args.push(String::from("--indent-style=tab")),
    }
    args
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::ValueEnum;

    fn every_language() -> impl Iterator<Item = LanguageName> {
        LanguageName::value_variants().iter().copied()
    }

    #[test]
    fn the_default_is_the_built_in_formatter_for_each_language() {
        let formatting = Formatting::default();
        for language in every_language() {
            let expected = match language {
                LanguageName::Rust => Some(Formatter::rustfmt()),
                LanguageName::Python => Some(Formatter::ruff()),
                LanguageName::Javascript
                | LanguageName::Jsx
                | LanguageName::Typescript
                | LanguageName::Tsx
                | LanguageName::Json => Some(Formatter::biome()),
                LanguageName::Toml => Some(Formatter::taplo()),
                LanguageName::Other => None,
            };
            assert_eq!(
                formatting.formatter(language),
                expected.as_ref(),
                "{language}"
            );
        }
    }

    #[test]
    fn off_has_no_formatter_for_any_language() {
        let formatting = Formatting::off();
        for language in every_language() {
            assert_eq!(formatting.formatter(language), None, "{language}");
        }
    }

    #[test]
    fn without_clears_one_language_and_leaves_the_rest() {
        let formatting = Formatting::default().without(LanguageName::Python);
        assert_eq!(formatting.formatter(LanguageName::Python), None);
        for language in every_language().filter(|language| *language != LanguageName::Python) {
            assert_eq!(
                formatting.formatter(language),
                Formatting::default().formatter(language),
                "{language}"
            );
        }
    }

    #[test]
    fn with_sets_one_language_and_leaves_the_rest() {
        let formatting = Formatting::off().with(LanguageName::Rust, Formatter::rustfmt());
        assert_eq!(
            formatting.formatter(LanguageName::Rust),
            Some(&Formatter::rustfmt())
        );
        for language in every_language().filter(|language| *language != LanguageName::Rust) {
            assert_eq!(formatting.formatter(language), None, "{language}");
        }

        // A later setting for the same language replaces the earlier one.
        let formatting = formatting.with(LanguageName::Rust, Formatter::taplo());
        assert_eq!(
            formatting.formatter(LanguageName::Rust),
            Some(&Formatter::taplo())
        );
    }

    #[test]
    fn set_sets_or_clears_in_place() {
        let mut formatting = Formatting::off();
        formatting
            .set(LanguageName::Toml, Some(Formatter::taplo()))
            .set(LanguageName::Json, Some(Formatter::biome()));
        assert_eq!(
            formatting,
            Formatting::off()
                .with(LanguageName::Toml, Formatter::taplo())
                .with(LanguageName::Json, Formatter::biome())
        );

        formatting.set(LanguageName::Toml, None);
        assert_eq!(
            formatting,
            Formatting::off().with(LanguageName::Json, Formatter::biome())
        );
    }

    #[test]
    fn taplo_formats_in_process() {
        assert_eq!(
            Formatter::taplo()
                .format("a=1\n", Path::new("Cargo.toml"))
                .unwrap(),
            "a = 1\n"
        );
    }
}
