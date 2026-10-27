#![allow(clippy::collapsible_if)]
#![deny(dead_code)]

mod editor;
mod education;
mod formatting;
mod indentation;
mod item;
mod languages;
mod layout;
mod searcher;
mod selector;
mod state;
mod tools;
mod validation;

#[cfg(test)]
mod tests;

use anyhow::{Result, anyhow, bail};
use async_compat::Compat;
use clap::{CommandFactory, FromArgMatches, Parser, ValueEnum};
use formatting::{Formatter, Formatting};
use languages::LanguageName;
use mcplease::{Cli, ServerConfig, server_info, traits::Dispatch, types::RequestContext};
use state::SemanticEditTools;
use std::{ffi::OsString, path::PathBuf, str::FromStr};
use tools::Tools;

const INSTRUCTIONS: &str = r#"Use edit to make a change: when the edit validates, it is written to disk immediately and the response shows the resulting diff. Review that diff — if the edit landed somewhere other than intended, retarget_edit moves it to a corrected anchor in one step, and undo_edit reverts it; both name the file the edit was made to. find_anchor checks where an anchor matches before editing. Paths are absolute, or relative to the server's root directory (by default, the directory it was started in).
"#;

#[derive(Parser)]
#[command(version, about)]
struct Args {
    /// The directory relative paths resolve against [default: the current directory]
    #[arg(long, global = true)]
    root: Option<PathBuf>,

    /// Format edited files: `LANGUAGE=FORMATTER` for one language, or `built-in`
    /// for each language's built-in formatter [default: no formatting]
    ///
    /// Repeatable. Each setting applies in turn, so a later one for a language
    /// replaces an earlier one. The formatters are rustfmt, ruff, biome and
    /// taplo, and `built-in` is rustfmt for rust, ruff for python, biome for
    /// javascript, jsx, typescript, tsx and json, and taplo for toml. rustfmt,
    /// ruff and biome must be on PATH; taplo runs in-process. A formatter
    /// reformats the whole file after each edit, and an edit to valid code that
    /// it fails on is rejected.
    // Not `global`: clap keeps a global option's occurrences from one side of
    // the subcommand only. `parse_argv` adds it to each subcommand instead, so
    // parse with that rather than with `Parser`'s methods.
    #[arg(long, value_name = "LANGUAGE=FORMATTER|built-in")]
    format: Vec<FormatSetting>,

    #[command(subcommand)]
    command: Cli<Tools>,
}

impl Args {
    /// Parse `argv`. `--format` goes before the subcommand, after it, or both,
    /// and every occurrence counts, in order.
    fn parse_argv<I, T>(argv: I) -> Result<Self, clap::Error>
    where
        I: IntoIterator<Item = T>,
        T: Into<OsString> + Clone,
    {
        let command = Self::command();
        let format = command
            .get_arguments()
            .find(|arg| arg.get_id() == "format")
            .cloned()
            .expect("`--format` is declared");
        let matches = command
            .mut_subcommands(|subcommand| subcommand.arg(format.clone()))
            .try_get_matches_from(argv)?;
        let after_subcommand = matches
            .subcommand()
            .and_then(|(_, subcommand)| subcommand.get_many::<FormatSetting>("format"))
            .into_iter()
            .flatten()
            .cloned();
        let mut args = Self::from_arg_matches(&matches)?;
        args.format.extend(after_subcommand);
        Ok(args)
    }

    /// The formatting the `--format` settings ask for, applied in turn: none
    /// without them.
    fn formatting(&self) -> Formatting {
        self.format
            .iter()
            .fold(Formatting::off(), |formatting, setting| match setting {
                FormatSetting::BuiltIn => Formatting::built_in(),
                FormatSetting::Language(language, formatter) => {
                    formatting.with(*language, formatter.clone())
                }
            })
    }
}

/// One `--format` setting.
#[derive(Clone, Debug, PartialEq, Eq)]
enum FormatSetting {
    /// `built-in`: each language's built-in formatter, as
    /// [`Formatting::built_in`].
    BuiltIn,
    /// `LANGUAGE=FORMATTER`: one language's formatter.
    Language(LanguageName, Formatter),
}

impl FromStr for FormatSetting {
    type Err = anyhow::Error;

    fn from_str(setting: &str) -> Result<Self> {
        if setting == "built-in" {
            return Ok(Self::BuiltIn);
        }
        let Some((language, formatter)) = setting.split_once('=') else {
            bail!("expected LANGUAGE=FORMATTER or built-in");
        };
        let language = <LanguageName as ValueEnum>::from_str(language, false)
            .map_err(|_| anyhow!("unknown language {language:?}; expected {}", languages()))?;
        Ok(Self::Language(language, formatter.parse()?))
    }
}

/// Every language's name, as `--language` takes it, for an error.
fn languages() -> String {
    let names = LanguageName::value_variants()
        .iter()
        .filter_map(ValueEnum::to_possible_value)
        .map(|name| name.get_name().to_owned())
        .collect::<Vec<_>>();
    let (last, rest) = names.split_last().expect("there are languages");
    format!("{} or {last}", rest.join(", "))
}

#[tokio::main]
async fn main() -> Result<()> {
    // stderr is the one place a stdio server may log.
    env_logger::init();
    let args = Args::parse_argv(std::env::args_os()).unwrap_or_else(|error| error.exit());
    let formatting = args.formatting();
    let Args { root, command, .. } = args;
    let root = match root {
        Some(root) => root,
        None => std::env::current_dir()?,
    };
    let state = SemanticEditTools::new(root)?.with_formatting(formatting);

    match command {
        Cli::Tool(tool) => {
            let output = tool.call_to_text(state, RequestContext::default()).await?;
            println!("{output}");
        }

        Cli::Serve => {
            let config = ServerConfig::new(server_info!()).with_instructions(INSTRUCTIONS);
            let stdin = Compat::new(tokio::io::stdin());
            let stdout = Compat::new(tokio::io::stdout());
            let mut calls = mcplease::calls::<Tools, _>(stdin, stdout, state, &config);
            while let Some(call) = calls.next().await {
                tokio::spawn(call?);
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod cli_tests {
    use super::*;
    use LanguageName::{Json, Rust, Toml};
    use clap::error::ErrorKind;

    fn parse(argv: &[&str]) -> Result<Args, clap::Error> {
        Args::parse_argv(std::iter::once("semantic-edit-mcp").chain(argv.iter().copied()))
    }

    fn formatting(argv: &[&str]) -> Formatting {
        parse(argv).unwrap().formatting()
    }

    #[test]
    fn without_format_nothing_is_formatted() {
        assert_eq!(formatting(&["serve"]), Formatting::off());
        assert_eq!(formatting(&["undo-edit", "a.rs"]), Formatting::off());
    }

    #[test]
    fn built_in_is_every_built_in_formatter() {
        assert_eq!(
            formatting(&["--format", "built-in", "serve"]),
            Formatting::built_in()
        );
    }

    #[test]
    fn a_language_setting_formats_that_language_alone() {
        assert_eq!(
            formatting(&["--format", "rust=rustfmt", "serve"]),
            Formatting::off().with(Rust, Formatter::rustfmt())
        );
    }

    #[test]
    fn settings_repeat_and_apply_in_turn() {
        assert_eq!(
            formatting(&["--format", "rust=rustfmt", "--format=toml=taplo", "serve"]),
            Formatting::off()
                .with(Rust, Formatter::rustfmt())
                .with(Toml, Formatter::taplo())
        );
        // A later setting for a language replaces an earlier one.
        assert_eq!(
            formatting(&["--format", "built-in", "--format", "json=taplo", "serve"]),
            Formatting::built_in().with(Json, Formatter::taplo())
        );
        assert_eq!(
            formatting(&["--format", "json=taplo", "--format", "built-in", "serve"]),
            Formatting::built_in()
        );
    }

    /// Like `--root`, `--format` goes before the subcommand or after it, for
    /// `serve` and for a one-shot tool, and the occurrences on both sides count.
    #[test]
    fn settings_go_on_either_side_of_the_subcommand() {
        let rust_and_toml = Formatting::off()
            .with(Rust, Formatter::rustfmt())
            .with(Toml, Formatter::taplo());
        for argv in [
            &[
                "--format",
                "rust=rustfmt",
                "--format",
                "toml=taplo",
                "serve",
            ][..],
            &[
                "serve",
                "--format",
                "rust=rustfmt",
                "--format",
                "toml=taplo",
            ],
            &[
                "--format",
                "rust=rustfmt",
                "serve",
                "--format",
                "toml=taplo",
            ],
            &[
                "--format",
                "rust=rustfmt",
                "edit",
                "a.rs",
                "replace",
                "x",
                "y",
                "--format",
                "toml=taplo",
            ],
            &[
                "undo-edit",
                "--format",
                "rust=rustfmt",
                "a.rs",
                "--format",
                "toml=taplo",
            ],
        ] {
            assert_eq!(formatting(argv), rust_and_toml, "{argv:?}");
        }

        // The rest of the command line parses as before.
        let args = parse(&[
            "--format", "built-in", "edit", "a.rs", "replace", "x", "y", "--root", "/tmp",
        ])
        .unwrap();
        assert!(matches!(args.command, Cli::Tool(Tools::Edit(_))));
        assert_eq!(args.root, Some(PathBuf::from("/tmp")));
    }

    #[test]
    fn unknown_names_are_errors() {
        let languages =
            "expected rust, json, toml, javascript, typescript, tsx, python, jsx or other";
        let formatters = "expected rustfmt, ruff, biome or taplo";
        for (setting, message) in [
            (
                "rsut=rustfmt",
                format!("unknown language \"rsut\"; {languages}"),
            ),
            (
                "Rust=rustfmt",
                format!("unknown language \"Rust\"; {languages}"),
            ),
            ("=rustfmt", format!("unknown language \"\"; {languages}")),
            (
                "rust=rustformat",
                format!("unknown formatter \"rustformat\"; {formatters}"),
            ),
            ("rust=", format!("unknown formatter \"\"; {formatters}")),
            (
                "rust",
                String::from("expected LANGUAGE=FORMATTER or built-in"),
            ),
            (
                "builtin",
                String::from("expected LANGUAGE=FORMATTER or built-in"),
            ),
        ] {
            for argv in [
                ["--format", setting, "serve"],
                ["serve", "--format", setting],
            ] {
                let Err(error) = parse(&argv) else {
                    panic!("{argv:?} parsed");
                };
                assert_eq!(error.kind(), ErrorKind::ValueValidation, "{argv:?}");
                let error = error.to_string();
                assert!(error.contains(&message), "{argv:?}: {error}");
            }
        }
    }

    #[test]
    fn format_needs_a_value() {
        let Err(error) = parse(&["serve", "--format"]) else {
            panic!("parsed");
        };
        assert_eq!(error.kind(), ErrorKind::InvalidValue);
    }
}
