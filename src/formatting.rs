//! Formatting: which formatter, if any, formats an edit, and how much of the
//! file it may change.
//!
//! Formatters are configuration, separate from languages: [`Formatting`] maps
//! each [`LanguageName`] to a [`Formatter`] or to none, and a [`Formatter`]
//! carries its own invocation. Formatting is off unless configured, and an edit
//! to a file whose language has no formatter is written as laid out. A
//! formatter changes only the edit unless configured to change the
//! [whole file](Formatting::whole_file).

mod command;
mod resolve;
mod rustfmt;

use crate::{indentation::Indentation, languages::LanguageName};
use anyhow::{Result, bail};
use enum_map::{EnumMap, enum_map};
use std::{
    ffi::OsString,
    fmt::{self, Display},
    path::Path,
    str::FromStr,
};

/// The formatter, if any, for each language's files, and whether it may change
/// the whole of an edited file.
///
/// A formatter formats each edit to a file of its language. An edit it fails on
/// is rejected, unless it fails on the file as it was before the edit too, or
/// can't run at all: then the edit is written as laid out, and the response says
/// why. A language without a formatter has its edits written as laid out. The
/// default is [`off`](Self::off), and [`built_in`](Self::built_in) is each
/// language's built-in formatter, both with [`whole_file`](Self::whole_file)
/// off.
///
/// A formatter formats the whole file, so it sees the edit in context, but
/// unless [`whole_file`](Self::whole_file) is set, only its changes to the
/// edit are kept:
///
/// - Its changes to the lines the edit changed, or to the line on either side
///   of them, are kept, and the rest of the file is left as it was. A change
///   that reaches across that boundary is kept whole.
/// - Where it moves part of the edit elsewhere in the file, as sorting imports
///   does, the moved text goes where it put it, so long as it changes nothing
///   else there. That change can be far from the edit, but it only adds the
///   edit's own text: every line already in the file stays as it was.
/// - If the changes kept can't stand without the rest, because the file isn't
///   valid without them, or because the formatter moved text between the edit
///   and the rest of the file in any other way (into the edit from elsewhere,
///   or out of it to where it changes other lines too), the edit is written as
///   laid out, and the response says so.
///
/// With `whole_file` set, all of its changes are kept, so an edit to a file the
/// formatter wouldn't leave as it is changes lines far from the edit.
#[derive(Clone, Debug, PartialEq, Eq, fieldwork::Fieldwork)]
pub struct Formatting {
    formatters: EnumMap<LanguageName, Option<Formatter>>,
    /// whether a formatter may change the whole of an edited file, and not only
    /// the edit: off unless set
    #[field(get, set, with)]
    whole_file: bool,
}

impl Default for Formatting {
    /// No formatter for any language, as [`off`](Formatting::off).
    fn default() -> Self {
        Self::off()
    }
}

impl Formatting {
    /// No formatter for any language: every edit is written as laid out. This is
    /// the default.
    pub fn off() -> Self {
        Self {
            formatters: EnumMap::default(),
            whole_file: false,
        }
    }

    /// Each language's built-in formatter: rustfmt for Rust, ruff for Python,
    /// biome for JavaScript, JSX, TypeScript, TSX and JSON, and taplo for TOML.
    /// Other files have none. rustfmt, ruff and biome are programs, found on
    /// `PATH` where edits run; taplo runs in-process.
    pub fn built_in() -> Self {
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
            whole_file: false,
        }
    }

    /// These settings, with `language` formatted by `formatter`.
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
    pub fn set(&mut self, language: LanguageName, formatter: Option<Formatter>) -> &mut Self {
        self.formatters[language] = formatter;
        self
    }

    /// The formatter for `language`'s files, if it has one.
    pub fn formatter(&self, language: LanguageName) -> Option<&Formatter> {
        self.formatters[language].as_ref()
    }
}

/// A formatter, which formats a file's whole source. Its [`Formatting`] says
/// how much of what it changes is kept: only its changes to the edit, unless it
/// is set to keep [all of them](Formatting::whole_file).
///
/// Each constructor carries its tool's invocation. A formatter program reads
/// the source on stdin and writes the result to stdout. It runs in the file's
/// directory, so it finds the project's own configuration, and it is given the
/// file's path where it takes one. One that can't run, because it isn't
/// installed or doesn't finish within 10 seconds, leaves the edit unformatted.
/// Parsing a built-in formatter's name (`"rustfmt"`, `"ruff"`, `"biome"` or
/// `"taplo"`) gives that formatter, and that name, or a command's program, is
/// how responses name it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Formatter {
    kind: Kind,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Kind {
    Rustfmt,
    Ruff,
    Biome,
    Taplo,
    Command { program: String, args: Vec<String> },
}

/// What a command's argument holds for the file's path.
const PATH_PLACEHOLDER: &str = "{path}";

/// Why a formatter gave no result for a source.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Failure {
    /// It couldn't run, or didn't finish in time, so it says nothing about the
    /// source: why, as a phrase.
    Unavailable(String),
    /// It ran and failed on the source: its error.
    Failed(String),
}

impl Formatter {
    /// rustfmt, for Rust, at the edition the nearest `Cargo.toml` declares, or
    /// 2024 when none does. Running in the file's directory, it finds the
    /// project's `rustfmt.toml`.
    pub fn rustfmt() -> Self {
        Self {
            kind: Kind::Rustfmt,
        }
    }

    /// `ruff format`, for Python, given the file's path. A file the project's
    /// configuration excludes is left as it is.
    pub fn ruff() -> Self {
        Self { kind: Kind::Ruff }
    }

    /// `biome format`, for JavaScript, JSX, TypeScript, TSX and JSON, given the
    /// file's path. A file no biome configuration governs keeps the indentation
    /// it already uses, rather than taking biome's default of tabs.
    pub fn biome() -> Self {
        Self { kind: Kind::Biome }
    }

    /// taplo, for TOML, with its default options. It runs in-process, so
    /// nothing needs to be installed.
    pub fn taplo() -> Self {
        Self { kind: Kind::Taplo }
    }

    /// `program` run with `args`: any program that reads the source on stdin
    /// and writes the result to stdout. `{path}` in an argument stands for the
    /// file's path. A program named without a directory is found on `PATH` as
    /// the platform finds programs; one named with a directory is taken
    /// relative to this process's working directory, not the file's.
    #[allow(dead_code, reason = "embedding API, used by library consumers")]
    pub fn command(
        program: impl Into<String>,
        args: impl IntoIterator<Item = impl Into<String>>,
    ) -> Self {
        Self {
            kind: Kind::Command {
                program: program.into(),
                args: args.into_iter().map(Into::into).collect(),
            },
        }
    }

    /// `source` formatted as the content of the file at `path`, whose content
    /// before the edit was `original`, or why there is no result.
    pub(crate) fn format(
        &self,
        source: &str,
        path: &Path,
        original: &str,
    ) -> Result<String, Failure> {
        let path = std::path::absolute(path).unwrap_or_else(|_| path.to_path_buf());
        let run = |program: &str, args: Vec<OsString>| {
            command::run(program, &args, path.parent(), source, command::TIMEOUT)
        };
        match &self.kind {
            Kind::Rustfmt => run(
                "rustfmt",
                os_strings(["--emit", "stdout", "--edition", rustfmt::edition(&path)]),
            ),
            Kind::Ruff => run("ruff", ruff_args(&path)),
            Kind::Biome => run("biome", biome_args(&path, original)),
            Kind::Taplo => Ok(taplo::formatter::format(
                source,
                taplo::formatter::Options::default(),
            )),
            Kind::Command { program, args } => run(
                program,
                args.iter().map(|arg| with_path(arg, &path)).collect(),
            ),
        }
    }
}

impl Display for Formatter {
    /// The built-in formatter's name, as it parses, or the command's program.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match &self.kind {
            Kind::Rustfmt => "rustfmt",
            Kind::Ruff => "ruff",
            Kind::Biome => "biome",
            Kind::Taplo => "taplo",
            Kind::Command { program, .. } => program,
        })
    }
}

impl FromStr for Formatter {
    type Err = anyhow::Error;

    /// The built-in formatter named `name`.
    fn from_str(name: &str) -> Result<Self> {
        Ok(match name {
            "rustfmt" => Self::rustfmt(),
            "ruff" => Self::ruff(),
            "biome" => Self::biome(),
            "taplo" => Self::taplo(),
            _ => bail!("unknown formatter {name:?}; expected rustfmt, ruff, biome or taplo"),
        })
    }
}

fn os_strings<const N: usize>(args: [&str; N]) -> Vec<OsString> {
    args.map(OsString::from).into()
}

/// `arg` with each `{path}` in it replaced by `path`.
fn with_path(arg: &str, path: &Path) -> OsString {
    let mut parts = arg.split(PATH_PLACEHOLDER);
    let mut replaced = OsString::from(parts.next().unwrap_or_default());
    for part in parts {
        replaced.push(path);
        replaced.push(part);
    }
    replaced
}

/// `ruff format` on stdin, as the file at `path`. ruff formats a file named on
/// its command line even when the project's configuration excludes it, unless
/// told to force the exclusion, as ruff's own pre-commit hook does.
fn ruff_args(path: &Path) -> Vec<OsString> {
    let mut args = os_strings(["format", "--force-exclude", "--stdin-filename"]);
    args.push(path.into());
    args.push("-".into());
    args
}

/// `biome format` on stdin, as the file at `path`. Indentation flags would
/// override the project's configuration, so they are passed only when there
/// is none, and then keep `original`'s indentation.
fn biome_args(path: &Path, original: &str) -> Vec<OsString> {
    let mut stdin_file_path = OsString::from("--stdin-file-path=");
    stdin_file_path.push(path);
    let mut args = vec![
        OsString::from("format"),
        stdin_file_path,
        OsString::from("--diagnostic-level=error"),
    ];
    let configured = std::env::var_os("BIOME_CONFIG_PATH").is_some_and(|path| !path.is_empty())
        || biome_configuration(path).is_some();
    if !configured {
        args.extend(biome_indentation(original));
    }
    args
}

/// The biome configuration file biome finds for the file at `path`: the
/// nearest one in its directory or above, which is where biome looks when it
/// runs there.
fn biome_configuration(path: &Path) -> Option<std::path::PathBuf> {
    const NAMES: [&str; 4] = ["biome.json", "biome.jsonc", ".biome.json", ".biome.jsonc"];
    path.ancestors()
        .skip(1)
        .flat_map(|dir| NAMES.map(|name| dir.join(name)))
        .find(|file| file.is_file())
}

/// biome's flags for the indentation `source` uses: two spaces if it shows none.
fn biome_indentation(source: &str) -> Vec<OsString> {
    match Indentation::determine(source).unwrap_or(Indentation::Spaces(2)) {
        Indentation::Spaces(spaces) => vec![
            OsString::from("--indent-style=space"),
            OsString::from(format!("--indent-width={spaces}")),
        ],
        Indentation::Tabs => vec![OsString::from("--indent-style=tab")],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::ValueEnum;

    fn every_language() -> impl Iterator<Item = LanguageName> {
        LanguageName::value_variants().iter().copied()
    }

    /// Running a program is configuration: nothing is formatted unless asked.
    #[test]
    fn the_default_formats_nothing() {
        assert_eq!(Formatting::default(), Formatting::off());
    }

    /// Edits are local unless whole-file formatting is asked for.
    #[test]
    fn only_the_edit_is_formatted_unless_whole_file_is_set() {
        for formatting in [
            Formatting::default(),
            Formatting::off(),
            Formatting::built_in(),
        ] {
            assert!(!formatting.whole_file(), "{formatting:?}");
        }

        let whole_file = Formatting::built_in().with_whole_file(true);
        assert!(whole_file.whole_file());
        assert_ne!(whole_file, Formatting::built_in());
        // It is separate from the formatters, which it leaves alone.
        for language in every_language() {
            assert_eq!(
                whole_file.formatter(language),
                Formatting::built_in().formatter(language),
                "{language}"
            );
        }

        let mut formatting = whole_file.with(LanguageName::Rust, Formatter::taplo());
        assert!(formatting.whole_file());
        formatting.set_whole_file(false);
        assert_eq!(
            formatting,
            Formatting::built_in().with(LanguageName::Rust, Formatter::taplo())
        );
    }

    #[test]
    fn built_in_is_the_built_in_formatter_for_each_language() {
        let formatting = Formatting::built_in();
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
        let formatting = Formatting::built_in().without(LanguageName::Python);
        assert_eq!(formatting.formatter(LanguageName::Python), None);
        for language in every_language().filter(|language| *language != LanguageName::Python) {
            assert_eq!(
                formatting.formatter(language),
                Formatting::built_in().formatter(language),
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
    fn a_formatter_parses_from_its_constructors_name() {
        for (name, formatter) in [
            ("rustfmt", Formatter::rustfmt()),
            ("ruff", Formatter::ruff()),
            ("biome", Formatter::biome()),
            ("taplo", Formatter::taplo()),
        ] {
            assert_eq!(name.parse::<Formatter>().unwrap(), formatter, "{name}");
        }
    }

    #[test]
    fn an_unknown_formatter_name_is_an_error_naming_the_known_ones() {
        for name in ["prettier", "Rustfmt", "rustfmt ", ""] {
            let error = name.parse::<Formatter>().unwrap_err().to_string();
            assert_eq!(
                error,
                format!("unknown formatter {name:?}; expected rustfmt, ruff, biome or taplo")
            );
        }
    }

    #[test]
    fn taplo_formats_in_process() {
        assert_eq!(
            Formatter::taplo()
                .format("a=1\n", Path::new("Cargo.toml"), "a=1\n")
                .unwrap(),
            "a = 1\n"
        );
    }

    /// Responses name the formatter as it parses, or by its program.
    #[test]
    fn a_formatter_displays_as_its_name_or_its_program() {
        for name in ["rustfmt", "ruff", "biome", "taplo"] {
            assert_eq!(name.parse::<Formatter>().unwrap().to_string(), name);
        }
        assert_eq!(
            Formatter::command("npx", ["prettier", "--stdin-filepath", "{path}"]).to_string(),
            "npx"
        );
        assert_eq!(
            Formatter::command("./scripts/format.sh", Vec::<String>::new()).to_string(),
            "./scripts/format.sh"
        );
    }

    #[test]
    fn path_replaces_each_placeholder_in_an_argument() {
        let path = Path::new("/project/src/main.rs");
        assert_eq!(with_path("{path}", path), "/project/src/main.rs");
        assert_eq!(
            with_path("--stdin-filepath={path}", path),
            "--stdin-filepath=/project/src/main.rs"
        );
        assert_eq!(
            with_path("{path}:{path}", path),
            "/project/src/main.rs:/project/src/main.rs"
        );
        assert_eq!(with_path("--check", path), "--check");
        assert_eq!(with_path("{pat}", path), "{pat}");
    }

    /// The file's path is ruff's to find the project's configuration and
    /// exclusions by.
    #[test]
    fn ruff_is_given_the_files_path_and_the_projects_exclusions() {
        assert_eq!(
            ruff_args(Path::new("/project/pkg/mod.py")),
            os_strings([
                "format",
                "--force-exclude",
                "--stdin-filename",
                "/project/pkg/mod.py",
                "-"
            ])
        );
    }

    /// Flags override a project's `biome.json`, so they are passed only for a
    /// file no configuration governs, at the indentation it had before the edit.
    #[test]
    fn biome_keeps_a_files_own_indentation_only_without_configuration() {
        let root = std::env::temp_dir().join(format!("semantic-edit-biome-{}", std::process::id()));
        let file = root.join("project/src/deep/app.ts");
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        let four = "class A {\n    a() {\n        return 1;\n    }\n}\n";
        let tabs = "class A {\n\ta() {}\n}\n";

        let without = (
            biome_indentation(four),
            biome_indentation(tabs),
            biome_indentation("a;\n"),
        );
        let mut found = vec![];
        for name in ["biome.json", "biome.jsonc", ".biome.json", ".biome.jsonc"] {
            let config = root.join("project").join(name);
            std::fs::write(&config, "{}").unwrap();
            found.push(biome_configuration(&file) == Some(config.clone()));
            std::fs::remove_file(&config).unwrap();
        }
        let after = biome_configuration(&file);
        std::fs::remove_dir_all(&root).ok();

        assert_eq!(
            without,
            (
                os_strings(["--indent-style=space", "--indent-width=4"]),
                os_strings(["--indent-style=tab"]),
                os_strings(["--indent-style=space", "--indent-width=2"]),
            )
        );
        assert_eq!(found, [true; 4]);
        assert_eq!(after, None);
    }

    /// A command's arguments name the file through `{path}`, and it runs in the
    /// file's directory.
    #[cfg(unix)]
    #[test]
    fn a_command_is_given_the_files_path_and_directory() {
        let dir =
            std::env::temp_dir().join(format!("semantic-edit-command-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let dir = std::fs::canonicalize(&dir).unwrap();
        let file = dir.join("main.rs");
        let formatter = Formatter::command(
            "sh",
            [
                "-c",
                "cat >/dev/null; printf '%s\\n%s\\n' \"$1\" \"$(pwd -P)\"",
                "sh",
                "--file={path}",
            ],
        );
        let result = formatter.format("fn main() {}\n", &file, "fn main() {}\n");
        std::fs::remove_dir_all(&dir).ok();
        assert_eq!(
            result.unwrap(),
            format!("--file={}\n{}\n", file.display(), dir.display())
        );
    }
}
