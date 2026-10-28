//! The settings' serialized shape, for an embedder that keeps them in its own
//! configuration, and its JSON schema.
//!
//! One shape serves [`Formatting`], settings on their own, and
//! [`FormattingLayer`], settings applied over others. They differ only in what a
//! key left out means: for [`Formatting`] its default, and for a layer the
//! setting before it.
//!
//! The schema's descriptions are the description strings here, and the doc
//! comments of the derived types' fields and variants: `Table`'s,
//! `CommandTable`'s, and `BuiltIn`'s in the parent module. An embedder inlines
//! them into its own schema and shows them to someone editing its
//! configuration, so they describe the settings in the configuration's terms,
//! never name this crate or the server it builds, and carry no rustdoc links. A
//! field whose schema is a reference to another definition has none: some
//! editors show only the definition's.

use super::{BuiltIn, Formatter, Formatting, Kind, formatted_languages};
use crate::languages::LanguageName;
use schemars::{JsonSchema, Schema, SchemaGenerator, generate::SchemaSettings, json_schema};
use serde::{
    Deserialize, Deserializer, Serialize, Serializer,
    de::{self, MapAccess, Unexpected, Visitor, value::MapAccessDeserializer},
    ser,
};
use std::{borrow::Cow, fmt};

/// [`Formatting`]'s description in the schema.
const FORMATTING_DESCRIPTION: &str = "\
Which formatter, if any, formats edits to each language's files, and how much \
of an edited file it may change. Every key is optional, and by default no \
language has a formatter: an edit is written as given, at the indentation of \
the code it replaces or goes next to. A formatter runs on the whole file, so it \
sees the edit in context. An edit it fails on is rejected, unless it also fails \
on the file without the edit, or can't run at all; then the edit is written \
unformatted, and its result says why.";

/// [`FormattingLayer`]'s description in the schema.
const LAYER_DESCRIPTION: &str = "\
Which formatter, if any, formats edits to each language's files, and how much \
of an edited file it may change, applied over the settings before these: a key \
set here replaces the earlier setting, and a key left out keeps it. Before any \
settings, no language has a formatter: an edit is written as given, at the \
indentation of the code it replaces or goes next to. A formatter runs on the \
whole file, so it sees the edit in context. An edit it fails on is rejected, \
unless it also fails on the file without the edit, or can't run at all; then \
the edit is written unformatted, and its result says why.";

/// The opening of [`Formatter`]'s description in the schema.
const FORMATTER_DESCRIPTION: &str = "\
A formatter: a built-in formatter's name, or a table with `command` and `args` \
to run any other program.";

/// The opening of a language's formatter's description in the schema.
const LANGUAGE_FORMATTER_DESCRIPTION: &str = "\
A language's formatter: a built-in formatter's name, a table with `command` and \
`args` to run any other program, or `false` for none.";

/// The rest of [`Formatter`]'s description, and a language's.
const FORMATTER_DETAILS: &str = "\
The built-in formatters are `\"rustfmt\"` for Rust, `\"ruff\"` for Python, \
`\"biome\"` for JavaScript, JSX, TypeScript, TSX and JSON, and `\"taplo\"` for \
TOML. A formatter program runs in the edited file's directory, so it finds the \
project's own configuration; one that isn't installed, or doesn't finish within \
10 seconds, leaves the edit unformatted.";

/// Formatting settings applied over others, as a configuration file's apply
/// over the files it builds on.
///
/// This is the settings' serialized shape, which [`Formatting`] shares: a table
/// with a key for each language that takes a formatter, named as
/// [`LanguageName`] displays it (`rust`, `json`, `toml`, `javascript`,
/// `typescript`, `tsx`, `python` and `jsx`), and two options. In TOML:
///
/// ```toml
/// built-in = true
/// whole-file = false
/// toml = false
/// python = { command = "uvx", args = ["ruff", "format", "--stdin-filename", "{path}", "-"] }
/// ```
///
/// - A language's value is a built-in formatter's name (`"rustfmt"`, `"ruff"`,
///   `"biome"` or `"taplo"`), a table naming a command, which becomes a
///   [`Formatter::command`] (`args` may be left out), or `false` for no
///   formatter.
/// - `built-in`: `true` gives every language the layer doesn't name its
///   built-in formatter, as [`Formatting::built_in`] does, and `false` gives
///   them none, as [`Formatting::off`] does.
/// - `whole-file`: [`Formatting::whole_file`].
///
/// Every key is optional, and one left out keeps the setting before it, so
/// [`Formatting::with_layer`] applies settings read from more than one place in
/// turn, each over the last. Applied over [`Formatting::off`], a layer is the
/// [`Formatting`] that deserializes from the same table.
///
/// Deserializing is strict: an unknown key, including a misspelled language or
/// `other`, an unknown formatter's name, a value of the wrong type, and `null`
/// are errors, each naming what was expected. A layer serializes to the keys it
/// sets.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FormattingLayer(Table);

impl Serialize for FormattingLayer {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.0.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for FormattingLayer {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_map(TableOnly).map(Self)
    }
}

impl JsonSchema for FormattingLayer {
    fn schema_name() -> Cow<'static, str> {
        "FormattingLayer".into()
    }

    fn schema_id() -> Cow<'static, str> {
        concat!(module_path!(), "::FormattingLayer").into()
    }

    fn json_schema(generator: &mut SchemaGenerator) -> Schema {
        Table::described(generator, LAYER_DESCRIPTION)
    }
}

// The settings' keys, which serde and schemars derive the shape from. A key's
// field is `None` where it is left out.
//
// Behind `FormattingLayer` rather than its fields, because a derived
// `Deserialize` would also read the settings from an array of their values in
// field order: the layer's reads only a table, then this from the table.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
struct Table {
    /// `true` gives each language not set here its built-in formatter: rustfmt
    /// for Rust, ruff for Python, biome for JavaScript, JSX, TypeScript, TSX and
    /// JSON, and taplo for TOML. `false` gives them none. rustfmt, ruff and biome
    /// are programs, found on `PATH`; taplo runs in-process.
    #[serde(deserialize_with = "present", skip_serializing_if = "Option::is_none")]
    #[schemars(with = "bool")]
    built_in: Option<bool>,

    /// Keep all of a formatter's changes to an edited file, not only its changes
    /// to the edit. Without it, a formatter's changes to the lines an edit
    /// changed, and to the line on either side of them, are kept, as is text of
    /// the edit's own that it moves elsewhere, as sorting imports does; the rest
    /// of the file stays as it was. If the changes kept can't stand without the
    /// rest, the edit is written unformatted. Default `false`.
    #[serde(deserialize_with = "present", skip_serializing_if = "Option::is_none")]
    #[schemars(with = "bool")]
    whole_file: Option<bool>,

    // Each language's formatter, in `LanguageName`'s order, which is the order
    // a misspelled key's error lists them in. `language` maps each
    // `LanguageName` to its field, so a new language doesn't compile until it
    // has one. They have no doc comments: their schema is a reference to
    // `LanguageFormatter`, whose description is theirs.
    #[serde(deserialize_with = "present", skip_serializing_if = "Option::is_none")]
    #[schemars(with = "LanguageFormatter")]
    rust: Option<LanguageFormatter>,

    #[serde(deserialize_with = "present", skip_serializing_if = "Option::is_none")]
    #[schemars(with = "LanguageFormatter")]
    json: Option<LanguageFormatter>,

    #[serde(deserialize_with = "present", skip_serializing_if = "Option::is_none")]
    #[schemars(with = "LanguageFormatter")]
    toml: Option<LanguageFormatter>,

    #[serde(deserialize_with = "present", skip_serializing_if = "Option::is_none")]
    #[schemars(with = "LanguageFormatter")]
    javascript: Option<LanguageFormatter>,

    #[serde(deserialize_with = "present", skip_serializing_if = "Option::is_none")]
    #[schemars(with = "LanguageFormatter")]
    typescript: Option<LanguageFormatter>,

    #[serde(deserialize_with = "present", skip_serializing_if = "Option::is_none")]
    #[schemars(with = "LanguageFormatter")]
    tsx: Option<LanguageFormatter>,

    #[serde(deserialize_with = "present", skip_serializing_if = "Option::is_none")]
    #[schemars(with = "LanguageFormatter")]
    python: Option<LanguageFormatter>,

    #[serde(deserialize_with = "present", skip_serializing_if = "Option::is_none")]
    #[schemars(with = "LanguageFormatter")]
    jsx: Option<LanguageFormatter>,
}

impl Table {
    /// What the table sets `language` to, if anything. `other` has no key.
    fn language(&self, language: LanguageName) -> Option<&LanguageFormatter> {
        let setting = match language {
            LanguageName::Rust => &self.rust,
            LanguageName::Json => &self.json,
            LanguageName::Toml => &self.toml,
            LanguageName::Javascript => &self.javascript,
            LanguageName::Typescript => &self.typescript,
            LanguageName::Tsx => &self.tsx,
            LanguageName::Python => &self.python,
            LanguageName::Jsx => &self.jsx,
            LanguageName::Other => return None,
        };
        setting.as_ref()
    }

    /// The table's schema, described as `description`. It is the whole of the
    /// schema of each type that serializes to a table, rather than a reference to
    /// a definition of its own, so each can say what a key left out means.
    fn described(generator: &mut SchemaGenerator, description: &str) -> Schema {
        let mut schema = Self::json_schema(generator);
        schema.insert("description".into(), description.into());
        schema
    }
}

impl From<&Formatting> for Table {
    /// The keys `formatting` serializes to: each language with a formatter, and
    /// `whole-file` when it is set. Applied over [`Formatting::off`], they are
    /// `formatting` again, less any formatter for [`LanguageName::Other`].
    fn from(formatting: &Formatting) -> Self {
        let set = |language| {
            formatting
                .formatter(language)
                .map(|formatter| LanguageFormatter(Some(formatter.clone())))
        };
        Self {
            built_in: None,
            whole_file: formatting.whole_file.then_some(true),
            rust: set(LanguageName::Rust),
            json: set(LanguageName::Json),
            toml: set(LanguageName::Toml),
            javascript: set(LanguageName::Javascript),
            typescript: set(LanguageName::Typescript),
            tsx: set(LanguageName::Tsx),
            python: set(LanguageName::Python),
            jsx: set(LanguageName::Jsx),
        }
    }
}

/// Reads a table, and nothing else.
struct TableOnly;

impl<'de> Visitor<'de> for TableOnly {
    type Value = Table;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("a table of formatting settings")
    }

    fn visit_map<A: MapAccess<'de>>(self, map: A) -> Result<Self::Value, A::Error> {
        Table::deserialize(MapAccessDeserializer::new(map))
    }
}

impl Formatting {
    /// These settings, with `layer`'s applied over them: its `built-in` first,
    /// if it sets it, then each language it sets, and its `whole-file`.
    /// Whatever it leaves out stays as it was.
    pub fn with_layer(mut self, layer: &FormattingLayer) -> Self {
        let FormattingLayer(table) = layer;
        if let Some(built_in) = table.built_in {
            let base = if built_in {
                Self::built_in()
            } else {
                Self::off()
            };
            self.formatters = base.formatters;
        }
        for language in formatted_languages() {
            if let Some(LanguageFormatter(formatter)) = table.language(language) {
                self.formatters[language] = formatter.clone();
            }
        }
        if let Some(whole_file) = table.whole_file {
            self.whole_file = whole_file;
        }
        self
    }

    /// The settings' JSON schema on its own: a root schema, draft 2020-12, with
    /// the definitions it refers to under `$defs`.
    ///
    /// An embedder that keeps the settings in its own configuration doesn't
    /// need this. Deriving [`JsonSchema`] for that configuration takes theirs
    /// along, as a `$ref` to a `Formatting` definition beside its own; a field
    /// holding [`FormattingLayer`] refers to a `FormattingLayer` definition,
    /// the same shape with a description for settings applied over others.
    /// None of the descriptions name this crate or its server, so they read
    /// correctly under the embedder's name.
    #[allow(dead_code, reason = "embedding API, used by library consumers")]
    pub fn schema() -> Schema {
        SchemaSettings::draft2020_12()
            .for_deserialize()
            .into_generator()
            .into_root_schema_for::<Self>()
    }
}

impl From<FormattingLayer> for Formatting {
    /// The settings `layer` makes on its own, applied over [`Formatting::off`].
    fn from(layer: FormattingLayer) -> Self {
        Self::off().with_layer(&layer)
    }
}

impl Serialize for Formatting {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        if self.formatters[LanguageName::Other].is_some() {
            return Err(ser::Error::custom(
                "a formatter for files of no known language has no setting to serialize to",
            ));
        }
        Table::from(self).serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for Formatting {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        FormattingLayer::deserialize(deserializer).map(Self::from)
    }
}

impl JsonSchema for Formatting {
    fn schema_name() -> Cow<'static, str> {
        "Formatting".into()
    }

    fn schema_id() -> Cow<'static, str> {
        concat!(module_path!(), "::Formatting").into()
    }

    fn json_schema(generator: &mut SchemaGenerator) -> Schema {
        Table::described(generator, FORMATTING_DESCRIPTION)
    }
}

/// A key's value, which `null` isn't: in JSON, as in TOML, a key with nothing
/// to set is left out.
fn present<'de, D: Deserializer<'de>, T: Deserialize<'de>>(
    deserializer: D,
) -> Result<Option<T>, D::Error> {
    T::deserialize(deserializer).map(Some)
}

/// A language's setting: a formatter, or none, spelled `false`.
#[derive(Clone, Debug, PartialEq, Eq)]
struct LanguageFormatter(Option<Formatter>);

impl Serialize for LanguageFormatter {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match &self.0 {
            Some(formatter) => formatter.serialize(serializer),
            None => serializer.serialize_bool(false),
        }
    }
}

impl<'de> Deserialize<'de> for LanguageFormatter {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer
            .deserialize_any(Spellings { or_none: true })
            .map(Self)
    }
}

impl JsonSchema for LanguageFormatter {
    fn schema_name() -> Cow<'static, str> {
        "LanguageFormatter".into()
    }

    fn schema_id() -> Cow<'static, str> {
        concat!(module_path!(), "::LanguageFormatter").into()
    }

    // The spellings are one union, rather than `Formatter`'s beside `false`,
    // and only the union is described: an editor shows the description of each
    // spelling it offers, one after another.
    fn json_schema(generator: &mut SchemaGenerator) -> Schema {
        json_schema!({
            "description": format!("{LANGUAGE_FORMATTER_DESCRIPTION} {FORMATTER_DETAILS}"),
            "anyOf": [
                generator.subschema_for::<BuiltIn>(),
                generator.subschema_for::<CommandTable>(),
                { "type": "boolean", "const": false },
            ],
        })
    }
}

impl Serialize for Formatter {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match &self.kind {
            Kind::BuiltIn(built_in) => serializer.serialize_str(built_in.name()),
            Kind::Command { program, args } => CommandTable {
                command: program.clone(),
                args: args.clone(),
            }
            .serialize(serializer),
        }
    }
}

impl<'de> Deserialize<'de> for Formatter {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer
            .deserialize_any(Spellings { or_none: false })?
            .ok_or_else(|| de::Error::custom("a formatter can't be `false`"))
    }
}

impl JsonSchema for Formatter {
    fn schema_name() -> Cow<'static, str> {
        "Formatter".into()
    }

    fn schema_id() -> Cow<'static, str> {
        concat!(module_path!(), "::Formatter").into()
    }

    fn json_schema(generator: &mut SchemaGenerator) -> Schema {
        json_schema!({
            "description": format!("{FORMATTER_DESCRIPTION} {FORMATTER_DETAILS}"),
            "anyOf": [
                generator.subschema_for::<BuiltIn>(),
                generator.subschema_for::<CommandTable>(),
            ],
        })
    }
}

/// The ways to write a formatter: a built-in formatter's name or a command's
/// table, and `false` for none where `or_none` allows it.
///
/// Written by hand rather than derived as an untagged enum, which would report
/// only that no spelling matched: this reports an unknown name as one, and a
/// misspelled key in a command's table by name.
struct Spellings {
    or_none: bool,
}

impl<'de> Visitor<'de> for Spellings {
    type Value = Option<Formatter>;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("a built-in formatter's name or a table with `command` and `args`")?;
        if self.or_none {
            f.write_str(", or `false` for none")?;
        }
        Ok(())
    }

    fn visit_bool<E: de::Error>(self, value: bool) -> Result<Self::Value, E> {
        match (self.or_none, value) {
            (true, false) => Ok(None),
            (true, true) => Err(E::invalid_value(Unexpected::Bool(value), &self)),
            (false, _) => Err(E::invalid_type(Unexpected::Bool(value), &self)),
        }
    }

    /// A built-in formatter's name. An unknown one is likelier a guess at
    /// another spelling, like `"prettier"` for a command or `"off"` for none,
    /// than a typo, so its error names those too.
    fn visit_str<E: de::Error>(self, name: &str) -> Result<Self::Value, E> {
        let Some(built_in) = BuiltIn::named(name) else {
            let command = "a table with `command` and `args` to run another program";
            return Err(E::custom(if self.or_none {
                format!("{}, {command}, or `false` for none", BuiltIn::unknown(name))
            } else {
                format!("{}, or {command}", BuiltIn::unknown(name))
            }));
        };
        Ok(Some(Formatter::built_in(built_in)))
    }

    fn visit_map<A: MapAccess<'de>>(self, map: A) -> Result<Self::Value, A::Error> {
        let CommandTable { command, args } =
            CommandTable::deserialize(MapAccessDeserializer::new(map))?;
        Ok(Some(Formatter::command(command, args)))
    }
}

// A command, as its table spells it. Like `BuiltIn`, it has no description of
// its own: it is one of the spellings of a formatter.
#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
#[schemars(rename = "FormatterCommand")]
struct CommandTable {
    /// The program to run. It reads the source on stdin and writes it,
    /// formatted, to stdout. A name without a directory is found on `PATH`; a
    /// path with one is relative to the working directory of the process making
    /// the edits, not to the file's.
    #[serde(deserialize_with = "program_name")]
    #[schemars(length(min = 1))]
    command: String,

    /// The program's arguments. `{path}` in an argument stands for the edited
    /// file's path.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    args: Vec<String>,
}

/// A command's program, which can't be empty.
fn program_name<'de, D: Deserializer<'de>>(deserializer: D) -> Result<String, D::Error> {
    let program = String::deserialize(deserializer)?;
    if program.is_empty() {
        return Err(de::Error::invalid_value(
            Unexpected::Str(&program),
            &"a program's name or path",
        ));
    }
    Ok(program)
}
