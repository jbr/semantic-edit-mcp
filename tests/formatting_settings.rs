//! The formatting settings as an embedder keeps them in its own configuration:
//! read from TOML through `toml_edit`'s serde layer, as efference reads its
//! configuration, or from JSON, and described by a JSON schema the embedder
//! inlines into its own.

use clap::ValueEnum;
use schemars::{JsonSchema, generate::SchemaSettings};
use semantic_edit_mcp::{Formatter, Formatting, FormattingLayer, LanguageName};
use serde::{Deserialize, de::DeserializeOwned};
use serde_json::{Value, json};
use std::path::Path;

use LanguageName::{Json, Jsx, Other, Python, Rust, Toml, Tsx, Typescript};

fn toml<T: DeserializeOwned>(text: &str) -> Result<T, String> {
    toml_edit::de::from_str(text).map_err(|error| error.to_string())
}

fn json<T: DeserializeOwned>(value: Value) -> Result<T, String> {
    serde_json::from_value(value).map_err(|error| error.to_string())
}

/// Every language a formatter can be set for: all but files of no known
/// language.
fn formatted_languages() -> impl Iterator<Item = LanguageName> {
    LanguageName::value_variants()
        .iter()
        .copied()
        .filter(|language| *language != Other)
}

fn uvx_ruff() -> Formatter {
    Formatter::command("uvx", ["ruff", "format", "--stdin-filename", "{path}", "-"])
}

/// A configuration that holds the settings, as an embedder's does.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct Config {
    #[serde(default)]
    formatting: Formatting,
}

#[test]
fn settings_read_naturally_in_toml() {
    let formatting: Formatting = toml(
        r#"
whole-file = false
rust = "rustfmt"
python = { command = "uvx", args = ["ruff", "format", "--stdin-filename", "{path}", "-"] }
"#,
    )
    .unwrap();
    assert_eq!(
        formatting,
        Formatting::off()
            .with(Rust, Formatter::rustfmt())
            .with(Python, uvx_ruff())
    );

    // In a table of the embedder's, a command's table is a table too.
    let config: Config = toml(
        r#"
[formatting]
whole-file = true
toml = "taplo"

[formatting.python]
command = "uvx"
args = ["ruff", "format", "--stdin-filename", "{path}", "-"]
"#,
    )
    .unwrap();
    assert_eq!(
        config.formatting,
        Formatting::off()
            .with(Toml, Formatter::taplo())
            .with(Python, uvx_ruff())
            .with_whole_file(true)
    );
}

/// The TOML examples in the file at `path`, a crate file's path, whose lines
/// open with `prefix`: `//! ` in a module's docs, `/// ` in an item's, or none.
fn toml_examples(path: &str, prefix: &str) -> Vec<String> {
    let text = std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join(path)).unwrap();
    let mut examples = vec![];
    let mut example: Option<String> = None;
    for line in text.lines() {
        let Some(line) = line.trim_start().strip_prefix(prefix.trim_end()) else {
            continue;
        };
        let line = line.strip_prefix(' ').unwrap_or(line);
        match (&mut example, line) {
            (None, "```toml") => example = Some(String::new()),
            (Some(_), "```") => examples.extend(example.take()),
            (Some(example), line) => {
                example.push_str(line);
                example.push('\n');
            }
            (None, _) => {}
        }
    }
    examples
}

/// The documentation's examples read as the settings they describe.
#[test]
fn the_documented_examples_read_as_described() {
    let readme = toml_examples("README.md", "");
    assert_eq!(readme.len(), 1, "{readme:?}");
    assert_eq!(
        toml::<Config>(&readme[0]).unwrap().formatting,
        Formatting::built_in()
            .with(Python, uvx_ruff())
            .without(Json)
    );

    let crate_docs = toml_examples("src/lib.rs", "//! ");
    assert_eq!(crate_docs.len(), 1, "{crate_docs:?}");
    assert_eq!(
        toml::<Config>(&crate_docs[0]).unwrap().formatting,
        Formatting::off()
            .with(Rust, Formatter::rustfmt())
            .with(Python, uvx_ruff())
    );

    let layer_docs = toml_examples("src/formatting/serialized.rs", "/// ");
    assert_eq!(layer_docs.len(), 1, "{layer_docs:?}");
    let layer: FormattingLayer = toml(&layer_docs[0]).unwrap();
    assert_eq!(
        Formatting::from(layer),
        Formatting::built_in()
            .without(Toml)
            .with(Python, uvx_ruff())
    );
}

#[test]
fn settings_read_the_same_in_json() {
    let formatting: Formatting = json(json!({
        "whole-file": false,
        "rust": "rustfmt",
        "python": {
            "command": "uvx",
            "args": ["ruff", "format", "--stdin-filename", "{path}", "-"],
        },
    }))
    .unwrap();
    assert_eq!(
        formatting,
        Formatting::off()
            .with(Rust, Formatter::rustfmt())
            .with(Python, uvx_ruff())
    );
}

/// Running a program is configuration, so settings that say nothing format
/// nothing, and the default says nothing.
#[test]
fn nothing_set_is_the_default() {
    assert_eq!(toml::<Formatting>("").unwrap(), Formatting::default());
    assert_eq!(
        json::<Formatting>(json!({})).unwrap(),
        Formatting::default()
    );
    assert_eq!(toml::<Config>("").unwrap().formatting, Formatting::off());

    assert_eq!(
        serde_json::to_value(Formatting::default()).unwrap(),
        json!({})
    );
    assert_eq!(
        toml_edit::ser::to_string(&Formatting::default()).unwrap(),
        ""
    );
}

#[test]
fn built_in_is_the_built_in_set() {
    assert_eq!(
        toml::<Formatting>("built-in = true").unwrap(),
        Formatting::built_in()
    );
    assert_eq!(
        toml::<Formatting>("built-in = false").unwrap(),
        Formatting::off()
    );

    // The languages beside it override it, wherever they are in the table.
    let except_python = Formatting::built_in().without(Python);
    assert_eq!(
        toml::<Formatting>("built-in = true\npython = false").unwrap(),
        except_python
    );
    assert_eq!(
        toml::<Formatting>("python = false\nbuilt-in = true").unwrap(),
        except_python
    );
    assert_eq!(
        json::<Formatting>(json!({"python": false, "built-in": true})).unwrap(),
        except_python
    );
    assert_eq!(
        toml::<Formatting>("built-in = false\ntoml = \"taplo\"").unwrap(),
        Formatting::off().with(Toml, Formatter::taplo())
    );

    // It sets formatters and nothing else.
    assert_eq!(
        toml::<Formatting>("built-in = true\nwhole-file = true").unwrap(),
        Formatting::built_in().with_whole_file(true)
    );
}

/// Settings serialize to the formatter of each language that has one, so the
/// built-in set is spelled out by name.
#[test]
fn the_built_in_set_serializes_by_name() {
    assert_eq!(
        serde_json::to_value(Formatting::built_in()).unwrap(),
        json!({
            "rust": "rustfmt",
            "json": "biome",
            "toml": "taplo",
            "javascript": "biome",
            "typescript": "biome",
            "tsx": "biome",
            "python": "ruff",
            "jsx": "biome",
        })
    );
}

#[test]
fn settings_serialize_to_the_shape_they_read_from() {
    let formatting = Formatting::off()
        .with(Rust, Formatter::rustfmt())
        .with(Python, Formatter::command("uvx", ["ruff", "format", "-"]))
        .with(Toml, Formatter::command("taplo-cli", Vec::<String>::new()))
        .with_whole_file(true);
    assert_eq!(
        toml_edit::ser::to_string(&formatting).unwrap(),
        r#"whole-file = true
rust = "rustfmt"
toml = { command = "taplo-cli" }
python = { command = "uvx", args = ["ruff", "format", "-"] }
"#
    );
    assert_eq!(
        serde_json::to_value(&formatting).unwrap(),
        json!({
            "whole-file": true,
            "rust": "rustfmt",
            "toml": {"command": "taplo-cli"},
            "python": {"command": "uvx", "args": ["ruff", "format", "-"]},
        })
    );
}

#[test]
fn settings_round_trip_through_toml_and_json() {
    for formatting in [
        Formatting::off(),
        Formatting::built_in(),
        Formatting::built_in().with_whole_file(true),
        Formatting::off().with_whole_file(true),
        Formatting::built_in()
            .without(Jsx)
            .with(Python, uvx_ruff())
            .with(Json, Formatter::taplo()),
        Formatting::off().with(Tsx, Formatter::command("./format.sh", Vec::<String>::new())),
    ] {
        let text = toml_edit::ser::to_string(&formatting).unwrap();
        assert_eq!(toml::<Formatting>(&text).unwrap(), formatting, "{text}");
        let value = serde_json::to_value(&formatting).unwrap();
        assert_eq!(
            json::<Formatting>(value.clone()).unwrap(),
            formatting,
            "{value}"
        );
    }
}

#[test]
fn each_language_is_named_as_it_displays() {
    for language in formatted_languages() {
        let name = language.to_string();
        assert_eq!(
            toml::<Formatting>(&format!("{name} = \"taplo\"")).unwrap(),
            Formatting::off().with(language, Formatter::taplo()),
            "{name}"
        );
        assert_eq!(
            toml::<Formatting>(&format!("built-in = true\n{name} = false")).unwrap(),
            Formatting::built_in().without(language),
            "{name}"
        );
    }
}

/// Files of no known language have no key, so settings that format them have
/// nothing to serialize to, and say so rather than drop it.
#[test]
fn a_formatter_for_other_files_does_not_serialize() {
    let formatting = Formatting::off().with(Other, Formatter::taplo());
    let error = serde_json::to_value(&formatting).unwrap_err().to_string();
    assert_eq!(
        error,
        "a formatter for files of no known language has no setting to serialize to"
    );
}

/// The errors read as `toml_edit` shows them, pointing at the key or value at
/// fault and naming what was expected.
#[test]
fn unknown_names_are_errors() {
    let keys = "expected one of `built-in`, `whole-file`, `rust`, `json`, `toml`, \
                `javascript`, `typescript`, `tsx`, `python`, `jsx`";
    for (text, error) in [
        (
            r#"rsut = "rustfmt""#,
            format!(
                "TOML parse error at line 1, column 1\n  |\n1 | rsut = \"rustfmt\"\n  \
                 | ^^^^\nunknown field `rsut`, {keys}\n"
            ),
        ),
        // Files of no known language take no formatter.
        (
            r#"other = "taplo""#,
            format!(
                "TOML parse error at line 1, column 1\n  |\n1 | other = \"taplo\"\n  \
                 | ^^^^^\nunknown field `other`, {keys}\n"
            ),
        ),
        (
            "whole_file = true",
            format!(
                "TOML parse error at line 1, column 1\n  |\n1 | whole_file = true\n  \
                 | ^^^^^^^^^^\nunknown field `whole_file`, {keys}\n"
            ),
        ),
        (
            r#"rust = "prettier""#,
            String::from(
                "TOML parse error at line 1, column 8\n  |\n1 | rust = \"prettier\"\n  \
                 |        ^^^^^^^^^^\nunknown formatter \"prettier\"; expected rustfmt, ruff, \
                 biome or taplo, a table with `command` and `args` to run another program, or \
                 `false` for none\n",
            ),
        ),
        (
            r#"python = { command = "uvx", arg = ["ruff"] }"#,
            String::from(
                "TOML parse error at line 1, column 29\n  |\n1 | python = { command = \"uvx\", \
                 arg = [\"ruff\"] }\n  |                             ^^^\nunknown field `arg`, \
                 expected `command` or `args`\n",
            ),
        ),
    ] {
        assert_eq!(toml::<Formatting>(text).unwrap_err(), error, "{text}");
    }

    // A built-in's name is exact, and the only string a formatter can be.
    for name in [
        "Rustfmt", "rustfmt ", "", "false", "off", "none", "built-in",
    ] {
        let error = toml::<Formatting>(&format!("rust = {name:?}")).unwrap_err();
        assert!(
            error.contains(&format!("unknown formatter {name:?}")),
            "{name:?}: {error}"
        );
    }
}

#[test]
fn values_of_the_wrong_type_are_errors() {
    let formatter = "expected a built-in formatter's name or a table with `command` and `args`, \
                     or `false` for none";
    for (text, error) in [
        (
            "rust = true",
            format!("invalid value: boolean `true`, {formatter}"),
        ),
        (
            "rust = 1",
            format!("invalid type: integer `1`, {formatter}"),
        ),
        (
            r#"rust = ["rustfmt"]"#,
            format!("invalid type: sequence, {formatter}"),
        ),
        (
            r#"whole-file = "true""#,
            String::from("invalid type: string \"true\", expected a boolean"),
        ),
        (
            "built-in = 1",
            String::from("invalid type: integer `1`, expected a boolean"),
        ),
        (
            "python = { args = [] }",
            String::from("missing field `command`"),
        ),
        (
            r#"python = { command = "" }"#,
            String::from("invalid value: string \"\", expected a program's name or path"),
        ),
        (
            "python = { command = 1 }",
            String::from("invalid type: integer `1`, expected a string"),
        ),
        (
            r#"python = { command = "uvx", args = "ruff" }"#,
            String::from("invalid type: string \"ruff\", expected a sequence"),
        ),
    ] {
        let message = toml::<Formatting>(text).unwrap_err();
        assert!(
            message.ends_with(&format!("\n{error}\n")),
            "{text}: {message}"
        );
    }

    // The settings are a table, not a value standing for one.
    for text in [
        r#"formatting = "built-in""#,
        "formatting = true",
        "formatting = [true, false]",
    ] {
        let message = toml::<Config>(text).unwrap_err();
        assert!(
            message.contains("expected a table of formatting settings"),
            "{text}: {message}"
        );
    }
}

/// TOML has no `null`; in JSON it isn't a way to leave a key out, or to say
/// none, which is `false`.
#[test]
fn null_is_an_error() {
    for value in [
        json!({"rust": null}),
        json!({"whole-file": null}),
        json!({"built-in": null}),
        json!({"python": {"command": null}}),
        json!(null),
    ] {
        assert!(json::<Formatting>(value.clone()).is_err(), "{value}");
        assert!(json::<FormattingLayer>(value.clone()).is_err(), "{value}");
    }
}

/// A layer is settings applied over others: what it sets replaces what came
/// before, and what it leaves out keeps it.
#[test]
fn a_layer_changes_only_what_it_sets() {
    let layer = |text| toml::<FormattingLayer>(text).unwrap();

    assert_eq!(
        Formatting::built_in().with_layer(&layer("python = false")),
        Formatting::built_in().without(Python)
    );
    assert_eq!(
        Formatting::built_in().with_layer(&layer("whole-file = true")),
        Formatting::built_in().with_whole_file(true)
    );
    assert_eq!(
        Formatting::built_in()
            .with_whole_file(true)
            .with_layer(&layer("built-in = false\nrust = \"rustfmt\"")),
        Formatting::off()
            .with(Rust, Formatter::rustfmt())
            .with_whole_file(true)
    );
    // `built-in` reaches every language the layer leaves out, commands
    // included.
    assert_eq!(
        Formatting::off()
            .with(Python, uvx_ruff())
            .with_layer(&layer("built-in = true")),
        Formatting::built_in()
    );
    for formatting in [
        Formatting::off(),
        Formatting::built_in().with_whole_file(true),
    ] {
        assert_eq!(
            formatting.clone().with_layer(&FormattingLayer::default()),
            formatting
        );
        assert_eq!(formatting.clone().with_layer(&layer("")), formatting);
    }

    // On its own, a layer is the settings the same table reads as.
    for text in [
        "",
        "built-in = true\njavascript = false",
        "typescript = \"biome\"\nwhole-file = true",
    ] {
        assert_eq!(
            Formatting::from(layer(text)),
            toml::<Formatting>(text).unwrap(),
            "{text}"
        );
    }
    assert_eq!(
        Formatting::from(layer("typescript = \"biome\"\nwhole-file = true")),
        Formatting::off()
            .with(Typescript, Formatter::biome())
            .with_whole_file(true)
    );
}

/// A layer keeps which keys it sets, `false` and `built-in` included.
#[test]
fn a_layer_round_trips_the_keys_it_sets() {
    for value in [
        json!({}),
        json!({"built-in": true, "javascript": false, "whole-file": false}),
        json!({"built-in": false, "rust": {"command": "rustfmt", "args": ["--edition", "2021"]}}),
    ] {
        let layer: FormattingLayer = json(value.clone()).unwrap();
        assert_eq!(serde_json::to_value(&layer).unwrap(), value);
        let text = toml_edit::ser::to_string(&layer).unwrap();
        assert_eq!(toml::<FormattingLayer>(&text).unwrap(), layer, "{text}");
    }
}

/// A standalone formatter reads as a language's value does, less `false`.
#[test]
fn a_formatter_reads_as_its_name_or_its_command() {
    assert_eq!(json::<Formatter>(json!("ruff")).unwrap(), Formatter::ruff());
    assert_eq!(
        json::<Formatter>(
            json!({"command": "uvx", "args": ["ruff", "format", "--stdin-filename", "{path}", "-"]})
        )
        .unwrap(),
        uvx_ruff()
    );
    assert_eq!(
        json::<Formatter>(json!({"command": "fmt"})).unwrap(),
        Formatter::command("fmt", Vec::<String>::new())
    );
    for formatter in [Formatter::biome(), uvx_ruff()] {
        let value = serde_json::to_value(&formatter).unwrap();
        assert_eq!(json::<Formatter>(value).unwrap(), formatter);
    }
    assert_eq!(
        json::<Formatter>(json!("off")).unwrap_err(),
        "unknown formatter \"off\"; expected rustfmt, ruff, biome or taplo, or a table with \
         `command` and `args` to run another program"
    );
    assert_eq!(
        json::<Formatter>(json!(false)).unwrap_err(),
        "invalid type: boolean `false`, expected a built-in formatter's name or a table with \
         `command` and `args`"
    );
}

/// Every description in `schema`, at any depth.
fn descriptions(schema: &Value) -> Vec<&str> {
    match schema {
        Value::Object(object) => object
            .iter()
            .flat_map(|(key, value)| match (key.as_str(), value) {
                ("description", Value::String(description)) => vec![description.as_str()],
                _ => descriptions(value),
            })
            .collect(),
        Value::Array(items) => items.iter().flat_map(descriptions).collect(),
        _ => vec![],
    }
}

/// The schema an embedder of both settings types publishes.
fn embedding_schema() -> Value {
    #[derive(JsonSchema)]
    #[allow(dead_code)]
    struct Embedder {
        formatting: Formatting,
        layer: FormattingLayer,
        formatter: Formatter,
    }
    SchemaSettings::default()
        .for_deserialize()
        .into_generator()
        .into_root_schema_for::<Embedder>()
        .to_value()
}

/// The descriptions appear in an embedder's schema, where this tool goes by the
/// embedder's name, so they never give it its own.
#[test]
fn the_schema_never_names_the_tool() {
    for schema in [Formatting::schema().to_value(), embedding_schema()] {
        let text = serde_json::to_string(&schema).unwrap().to_lowercase();
        for name in ["semantic", "semantic-edit", "semantic_edit"] {
            assert!(!text.contains(name), "{name} in {text}");
        }
    }
}

/// The descriptions are written for someone editing a configuration file, so
/// none of rustdoc's syntax reaches them.
#[test]
fn the_schema_descriptions_are_prose() {
    let schema = embedding_schema();
    let descriptions = descriptions(&schema);
    assert!(descriptions.len() > 5, "{descriptions:?}");
    for description in descriptions {
        for artifact in ["[`", "](", "Self", "crate::", "::"] {
            assert!(
                !description.contains(artifact),
                "{artifact:?} in {description:?}"
            );
        }
    }
}

/// An embedder's configuration takes the settings' schema by deriving its own:
/// a reference to their definitions, which join its own.
#[test]
fn an_embedder_takes_the_schema_by_reference() {
    let schema = SchemaSettings::default()
        .for_deserialize()
        .into_generator()
        .into_root_schema_for::<Config>()
        .to_value();
    assert_eq!(
        schema["properties"]["formatting"],
        json!({"$ref": "#/$defs/Formatting", "default": {}})
    );
    let definitions: Vec<&str> = schema["$defs"]
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    assert_eq!(
        definitions,
        [
            "BuiltInFormatter",
            "FormatterCommand",
            "Formatting",
            "LanguageFormatter"
        ]
    );
    // The definition is the root schema's, less what only a root has.
    let mut root = Formatting::schema().to_value();
    let root = root.as_object_mut().unwrap();
    for key in ["$schema", "title", "$defs"] {
        root.remove(key);
    }
    assert_eq!(schema["$defs"]["Formatting"], Value::Object(root.clone()));

    // A layer is the same table, described as applying over others.
    let embedder = embedding_schema();
    let layer = &embedder["$defs"]["FormattingLayer"];
    let formatting = &embedder["$defs"]["Formatting"];
    assert_eq!(layer["properties"], formatting["properties"]);
    assert_ne!(layer["description"], formatting["description"]);
    assert_eq!(
        embedder["properties"]["layer"],
        json!({"$ref": "#/$defs/FormattingLayer"})
    );
}

/// The schema admits what deserializes: its keys, and the names that parse.
#[test]
fn the_schema_describes_what_deserializes() {
    let schema = Formatting::schema().to_value();
    let keys: Vec<&str> = schema["properties"]
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    let mut expected = vec!["built-in".to_owned(), "whole-file".to_owned()];
    expected.extend(formatted_languages().map(|language| language.to_string()));
    let mut keys = keys;
    keys.sort_unstable();
    expected.sort_unstable();
    assert_eq!(keys, expected);
    assert_eq!(schema["additionalProperties"], json!(false));

    let names: Vec<&str> = schema["$defs"]["BuiltInFormatter"]["oneOf"]
        .as_array()
        .unwrap()
        .iter()
        .map(|name| name["const"].as_str().unwrap())
        .collect();
    assert_eq!(names, ["rustfmt", "ruff", "biome", "taplo"]);
    for name in names {
        let formatter: Formatter = name.parse().unwrap();
        assert_eq!(formatter.to_string(), name);
        assert_eq!(json::<Formatter>(json!(name)).unwrap(), formatter);
    }
}

/// The schema is checked in, so a change to it shows in review. Regenerate it
/// with `UPDATE_SNAPSHOTS=1 cargo test`, as the snapshot suite is.
#[test]
fn the_schema_is_committed() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/formatting.schema.json");
    let mut generated = serde_json::to_string_pretty(&Formatting::schema()).unwrap();
    generated.push('\n');
    if std::env::var_os("UPDATE_SNAPSHOTS").is_some() {
        std::fs::write(&path, &generated).unwrap();
        return;
    }
    let committed = std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("{}: {error}", path.display()));
    assert!(
        committed == generated,
        "{} differs from the generated schema; regenerate it with UPDATE_SNAPSHOTS=1 cargo test",
        path.display()
    );
}
