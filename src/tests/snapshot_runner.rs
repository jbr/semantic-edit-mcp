use crate::formatting::{Formatter, Formatting};
use crate::languages::LanguageName;
use crate::state::SemanticEditTools;
use crate::tools::Tools;
use anyhow::{Context, Error, Result, anyhow, bail};
use clap::ValueEnum;
use diffy::{DiffOptions, PatchFormatter};
use mcplease::traits::Dispatch;
use mcplease::types::RequestContext;
use serde_json::{Map, Value};
use std::fs;
use std::path::{Path, PathBuf};

/// A test directory, or any directory above one, may hold this file to set the
/// formatting of every test beneath it. See [`formatting_for`].
const FORMATTING_FILE: &str = "formatting.json";

pub struct SnapshotRunner {
    update_mode: bool,
    state: SemanticEditTools,
    test_filter: Option<String>,
}

#[derive(Debug, Clone)]
pub struct SnapshotTest {
    pub name: String,
    pub input_path: Option<PathBuf>,
    pub args_path: PathBuf,
    pub response_path: PathBuf,
    pub output_path: Option<PathBuf>,
    /// Every [`FORMATTING_FILE`] from the snapshot root down to the test's own
    /// directory, outermost first.
    pub formatting_paths: Vec<PathBuf>,
}

#[derive(Debug)]
pub struct SnapshotResult {
    pub test: SnapshotTest,
    pub actual_response: String,
    pub expected_response: Option<String>,
    pub actual_output: Option<String>, // NEW: Actual file content after transformation
    pub expected_output: Option<String>, // NEW: Expected file content
    pub error: Option<String>,
    pub response_matches: bool,
    pub output_matches: bool,
}

#[derive(Debug)]
struct SnapshotExecutionResult {
    response: String,
    output: Option<String>,
}

struct ArgsDotJson;

impl ArgsDotJson {
    fn to_tools(args: Value, input_path: Option<&Path>, _args_path: &Path) -> Result<Vec<Tools>> {
        let mut tool_calls = match args {
            Value::Array(a) => a,
            o @ Value::Object(_) => vec![o],
            _ => panic!(),
        };

        for tool in &mut tool_calls {
            // Every tool that names a file names the fixture's input.
            if matches!(
                tool["name"].as_str(),
                Some("edit" | "undo_edit" | "retarget_edit")
            ) {
                if let Some(input_path) = &input_path {
                    tool.get_mut("arguments")
                        .unwrap()
                        .as_object_mut()
                        .unwrap()
                        .insert(
                            "file_path".to_string(),
                            Value::String(
                                input_path
                                    .file_name()
                                    .unwrap()
                                    .to_string_lossy()
                                    .to_string(),
                            ),
                        );
                }
            }
        }

        tool_calls
            .into_iter()
            .map(serde_json::from_value)
            .collect::<Result<_, _>>()
            .map_err(Error::from)
    }
}

impl SnapshotRunner {
    pub fn new(update_mode: bool, test_filter: Option<String>) -> Result<Self> {
        let state = SemanticEditTools::new(PathBuf::from("."))?;
        Ok(Self {
            update_mode,
            state,
            test_filter,
        })
    }

    fn reset_state(&mut self, base_path: PathBuf, formatting: Formatting) -> Result<()> {
        self.state = SemanticEditTools::new(base_path)?.with_formatting(formatting);
        Ok(())
    }

    /// Discover all snapshot tests in the tests/snapshots directory
    pub fn discover_tests(&self) -> Result<Vec<SnapshotTest>> {
        let snapshots_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/snapshots");
        let mut tests = Vec::new();

        Self::discover_tests_recursive(&snapshots_dir, &snapshots_dir, &[], &mut tests)?;

        tests.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(tests)
    }

    /// Filter tests by the TEST_FILTER environment variable: patterns separated
    /// by commas, each matched by [`filter_matches`].
    fn filter_tests(&self, tests: Vec<SnapshotTest>) -> Vec<SnapshotTest> {
        if let Some(filter) = &self.test_filter {
            let patterns: Vec<&str> = filter.split(',').map(|s| s.trim()).collect();

            tests
                .into_iter()
                .filter(|test| {
                    patterns
                        .iter()
                        .any(|pattern| filter_matches(pattern, &test.name))
                })
                .collect()
        } else {
            tests
        }
    }

    /// Collect the tests under `dir`. `formatting` is the [`FORMATTING_FILE`]s
    /// above it, outermost first.
    fn discover_tests_recursive(
        base: &Path,
        dir: &Path,
        formatting: &[PathBuf],
        tests: &mut Vec<SnapshotTest>,
    ) -> Result<()> {
        let formatting = with_formatting_file(formatting, dir);
        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();

            if path.is_dir() {
                // Check if this directory contains a complete test (input + args files)
                let args_path = path.join("args.json");
                let response_path = path.join("response.txt");

                let input_path = fs::read_dir(&path)?.into_iter().find_map(|entry| {
                    entry.ok().and_then(|entry| {
                        if entry.path().is_file()
                            && entry.path().file_stem().and_then(|x| x.to_str()) == Some("input")
                        {
                            Some(entry.path())
                        } else {
                            None
                        }
                    })
                });

                if args_path.exists() {
                    let output_path = input_path.as_ref().map(|input_path| {
                        let mut output_path = path.join("output");
                        if let Some(extension) = input_path.extension() {
                            output_path.set_extension(extension);
                        }
                        output_path
                    });

                    let test_name = path
                        .strip_prefix(base)
                        .unwrap_or(&path)
                        .to_string_lossy()
                        .replace('/', "::");

                    tests.push(SnapshotTest {
                        name: test_name,
                        input_path,
                        args_path,
                        response_path,
                        output_path,
                        formatting_paths: with_formatting_file(&formatting, &path),
                    });
                } else {
                    // Recurse into subdirectories
                    Self::discover_tests_recursive(base, &path, &formatting, tests)?;
                }
            }
        }

        Ok(())
    }

    /// Run a single snapshot test
    pub fn run_test(&mut self, test: SnapshotTest) -> SnapshotResult {
        let result = match self.execute_test(&test) {
            Ok(result) => result,
            Err(e) => {
                return SnapshotResult {
                    test,
                    actual_response: String::new(),
                    expected_response: None,
                    // With its causes: a bad formatting file is named by its
                    // context and explained by its cause.
                    error: Some(format!("{e:#}")),
                    actual_output: None,
                    expected_output: None,
                    response_matches: false,
                    output_matches: false,
                };
            }
        };
        if self.update_mode {
            self.update_snapshot(result, test)
        } else {
            self.compare_snapshot(result, test)
        }
    }

    fn update_snapshot(
        &self,
        result: SnapshotExecutionResult,
        test: SnapshotTest,
    ) -> SnapshotResult {
        let SnapshotExecutionResult { response, output } = result;
        let mut result = SnapshotResult {
            test,
            actual_response: response,
            expected_response: None,
            actual_output: output,
            expected_output: None,
            error: None,
            response_matches: true,
            output_matches: true,
        };
        // Write the actual response as the new expected response
        if let Err(e) = std::fs::write(&result.test.response_path, &result.actual_response) {
            result.error = Some(format!("Failed to write expected output: {e}"));
            return result;
        }

        // Write the actual output as the new expected output
        if let Some(output) = &result.actual_output {
            let Some(output_path) = &result.test.output_path else {
                result.error = Some("output without input is unexpected".to_string());
                return result;
            };
            if let Err(e) = std::fs::write(output_path, output) {
                result.error = Some(format!("Failed to write expected output: {e}"));
                return result;
            }
        } else if let Some(output_path) = &result.test.output_path {
            if let Ok(true) = output_path.try_exists() {
                // If there is no expected output but the file exists, delete the file
                if let Err(e) = std::fs::remove_file(output_path) {
                    result.error =
                        Some(format!("No output expected, but was unable to delete: {e}"));
                    return result;
                }
            }
        }

        result
    }

    fn compare_snapshot(
        &self,
        result: SnapshotExecutionResult,
        test: SnapshotTest,
    ) -> SnapshotResult {
        let SnapshotExecutionResult { response, output } = result;
        let mut result = SnapshotResult {
            test,
            actual_response: response,
            expected_response: None,
            actual_output: output,
            expected_output: None,
            error: None,
            response_matches: false,
            output_matches: false,
        };

        // Compare with expected output
        result.expected_response =
            Some(match std::fs::read_to_string(&result.test.response_path) {
                Ok(content) => content,
                Err(_) => {
                    result.error = Some(
                        "Response file not found. Run with --update to create it.".to_string(),
                    );
                    return result;
                }
            });

        result.expected_output = if let Some(output_path) = &result.test.output_path {
            std::fs::read_to_string(output_path).ok()
        } else {
            None
        };

        result.response_matches = result
            .expected_response
            .as_deref()
            .is_some_and(|expected| expected.trim() == result.actual_response.trim());

        result.output_matches = match (
            result.actual_output.as_deref(),
            result.expected_output.as_deref(),
        ) {
            (Some(actual), Some(expected)) => actual.trim() == expected.trim(),
            (None, None) => true,
            _ => false,
        };

        result
    }

    /// Execute a single test and return the tool output.
    ///
    /// Runs against a scratch copy of the input file: edits persist on
    /// success, so running in place would mutate the checked-in inputs, and
    /// capturing writes instead of performing them would diverge from
    /// production behavior (sequential edits accumulate on disk, and undo and
    /// retarget verify the file's current content before acting).
    fn execute_test(&mut self, test: &SnapshotTest) -> Result<SnapshotExecutionResult> {
        let scratch_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("target/snapshot-scratch")
            .join(test.name.replace("::", "/"));
        if scratch_dir.exists() {
            fs::remove_dir_all(&scratch_dir)?;
        }
        fs::create_dir_all(&scratch_dir)?;

        let input = match &test.input_path {
            Some(input_path) => {
                let file_name = input_path.file_name().unwrap();
                let content = fs::read_to_string(input_path)?;
                fs::write(scratch_dir.join(file_name), &content)?;
                Some((scratch_dir.join(file_name), content))
            }
            None => None,
        };

        self.reset_state(scratch_dir, formatting_for(&test.formatting_paths)?)?;

        // Read the arguments
        let args_content = fs::read_to_string(&test.args_path)?;
        let tool_calls = ArgsDotJson::to_tools(
            serde_json::from_str(&args_content)?,
            test.input_path.as_deref(),
            &test.args_path,
        )?;

        let mut snapshot_execution_result = SnapshotExecutionResult {
            response: String::new(),
            output: None,
        };

        // Tool bodies run on tokio's blocking pool.
        let runtime = tokio::runtime::Builder::new_current_thread().build()?;
        for tool in tool_calls {
            snapshot_execution_result
                .response
                .push_str("=== snapshot test tool call: ");
            snapshot_execution_result.response.push_str(tool.name());
            snapshot_execution_result.response.push_str(" ===\n");

            match runtime.block_on(tool.call_to_text(self.state.clone(), RequestContext::default()))
            {
                Ok(response) => snapshot_execution_result.response.push_str(&response),
                // Marked, so the snapshot records *whether* the text came back as
                // an error as well as what it said. Without the marker a rejection
                // that moves between the success and error channels leaves every
                // response.txt byte-identical, and the suite cannot see the move.
                Err(err) => {
                    snapshot_execution_result
                        .response
                        .push_str("=== error ===\n");
                    snapshot_execution_result
                        .response
                        .push_str(&err.to_string());
                }
            }
            snapshot_execution_result.response.push('\n');
        }

        // The test's output is the scratch file's final content — but only
        // when the tool calls left a net change, so no-op and fully-reverted
        // sequences assert "no output file" exactly like failed edits do.
        if let Some((scratch_path, input_content)) = input {
            let final_content = fs::read_to_string(scratch_path)?;
            if final_content != input_content {
                snapshot_execution_result.output = Some(final_content);
            }
        }

        Ok(snapshot_execution_result)
    }

    /// Run all discovered tests (filtered if TEST_FILTER is set)
    pub fn run_all_tests(&mut self) -> Result<Vec<SnapshotResult>> {
        let all_tests = self.discover_tests()?;
        let tests = self.filter_tests(all_tests);
        assert!(
            !tests.is_empty(),
            "no snapshot test matches TEST_FILTER={}",
            self.test_filter.as_deref().unwrap_or_default()
        );

        if let Some(filter) = &self.test_filter {
            println!("🔍 Running filtered tests: {filter}");
            println!("   Found {} matching test(s)", tests.len());
        }

        let mut results = Vec::new();

        for test in tests {
            let result = self.run_test(test);
            results.push(result);
        }

        Ok(results)
    }

    /// Print a summary of test results
    pub fn print_summary(&self, results: &[SnapshotResult]) {
        let total = results.len();
        let passed = results
            .iter()
            .filter(|r| r.response_matches && r.output_matches)
            .count();
        let failed = total - passed;

        let mut diff_options = DiffOptions::new();
        diff_options.set_original_filename("expected");
        diff_options.set_modified_filename("actual");
        let f = PatchFormatter::new()
            .with_color()
            .missing_newline_message(false);

        println!("\n===📊 Snapshot Test Summary===");
        println!("  Total:  {total}");
        println!("  Passed: {passed}");
        println!("  Failed: {failed}");

        if self.update_mode {
            println!("  Mode:   UPDATE (expected outputs written)");
        } else {
            println!("  Mode:   VERIFY");
        }

        if let Some(filter) = &self.test_filter {
            println!("  Filter: {filter}");
        }

        if passed > 0 {
            println!("\n✅ Passed tests:");
            for result in results
                .iter()
                .filter(|r| r.response_matches && r.output_matches)
            {
                println!("  • {}", result.test.name);
            }
        }

        if failed > 0 {
            println!("\n❌ Failed tests:");
            for result in results
                .iter()
                .filter(|r| !r.response_matches || !r.output_matches)
            {
                println!("  • {}", result.test.name);
            }

            println!("\n\n=== Failed tests details ===\n");

            for result in results
                .iter()
                .filter(|r| !r.response_matches || !r.output_matches)
            {
                println!("❌ {}", result.test.name);
                println!(
                    "To target just this test, run `TEST_FILTER={} cargo test`",
                    result.test.name
                );
                println!(
                    "To update snapshot for just this test, run `UPDATE_SNAPSHOTS=1 TEST_FILTER={} cargo test`",
                    result.test.name
                );
                if let Some(error) = &result.error {
                    println!("Error:\n{error}");
                } else {
                    if !result.response_matches {
                        println!("Expected response differs from actual output");
                        let expected_response =
                            result.expected_response.as_deref().unwrap_or_default();
                        let patch =
                            diff_options.create_patch(expected_response, &result.actual_response);
                        println!("\n***RESPONSE DIFF***\n\n{}", f.fmt_patch(&patch));
                    }

                    if !result.output_matches {
                        println!("Expected output differs from actual output");
                        let expected_output = result.expected_output.as_deref().unwrap_or_default();
                        let actual_output = result.actual_output.as_deref().unwrap_or_default();
                        let patch = diff_options.create_patch(expected_output, actual_output);
                        println!("\n***OUTPUT DIFF***\n\n{}", f.fmt_patch(&patch));
                    }
                }

                println!("\n\n");
            }
        }
    }
}

/// Whether the TEST_FILTER pattern `pattern` selects the test named `name`.
///
/// - A bare category (no `::`, e.g. `rust`) selects every test whose name
///   begins with it.
/// - A `::` path (e.g. `rust::repair`, or a full name like
///   `rust::repair::insert`) selects the test of that name and every test
///   beneath it, on `::` boundaries: `rust::repair` doesn't select
///   `rust::repairs::insert`. So the full name the failure summary prints
///   selects exactly that test.
fn filter_matches(pattern: &str, name: &str) -> bool {
    if pattern.is_empty() {
        false
    } else if pattern.contains("::") {
        name.strip_prefix(pattern)
            .is_some_and(|rest| rest.is_empty() || rest.starts_with("::"))
    } else {
        name.starts_with(pattern)
    }
}

/// `formatting` with `dir`'s own [`FORMATTING_FILE`], if it has one, as the
/// innermost layer.
fn with_formatting_file(formatting: &[PathBuf], dir: &Path) -> Vec<PathBuf> {
    let mut layers = formatting.to_vec();
    let file = dir.join(FORMATTING_FILE);
    if file.is_file() {
        layers.push(file);
    }
    layers
}

/// The formatting a test runs with: the default, which is off, overridden by
/// each of its [`FORMATTING_FILE`]s in turn, outermost first. The one at the
/// snapshot root turns on [`Formatting::built_in`], so a test is formatted
/// unless a file nearer to it says otherwise.
///
/// A formatting file is a JSON object from a language (named as `args.json`
/// names one: `rust`, `typescript`, …) or `"*"`, every language, to a
/// formatter (`"rustfmt"`, `"ruff"`, `"biome"`, `"taplo"`) or `null` for none.
/// `"*"` applies before the languages beside it.
///
/// - `{"*": null}`: no formatter for any language
/// - `{"rust": null}`: none for Rust, the rest as inherited
/// - `{"toml": "taplo"}`: taplo for TOML, the rest as inherited
fn formatting_for(layers: &[PathBuf]) -> Result<Formatting> {
    let mut formatting = Formatting::default();
    for path in layers {
        let text = fs::read_to_string(path)?;
        serde_json::from_str(&text)
            .map_err(Error::from)
            .and_then(|layer| apply_formatting_layer(&mut formatting, &layer))
            .with_context(|| format!("in {}", path.display()))?;
    }
    Ok(formatting)
}

fn apply_formatting_layer(formatting: &mut Formatting, layer: &Map<String, Value>) -> Result<()> {
    if let Some(value) = layer.get("*") {
        let formatter = parse_formatter(value)?;
        for language in LanguageName::value_variants() {
            formatting.set(*language, formatter.clone());
        }
    }
    for (key, value) in layer.iter().filter(|(key, _)| *key != "*") {
        formatting.set(parse_language(key)?, parse_formatter(value)?);
    }
    Ok(())
}

/// A language as `args.json` names one. Unlike deserializing a
/// [`LanguageName`], which reads any unknown name as `other`, a misspelled
/// language is an error.
fn parse_language(name: &str) -> Result<LanguageName> {
    LanguageName::value_variants()
        .iter()
        .copied()
        .find(|language| language.to_string() == name)
        .ok_or_else(|| anyhow!("unknown language {name:?}"))
}

fn parse_formatter(value: &Value) -> Result<Option<Formatter>> {
    match value {
        Value::Null => Ok(None),
        Value::String(name) => Ok(Some(name.parse()?)),
        other => bail!("expected a formatter name or null, found {other}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_path_selects_the_test_and_every_test_beneath_it() {
        assert!(filter_matches("rust::repair", "rust::repair"));
        assert!(filter_matches("rust::repair", "rust::repair::insert"));
        assert!(filter_matches(
            "rust::repair",
            "rust::repair::deeper::insert"
        ));
        assert!(!filter_matches("rust::repair", "rust::repairs"));
        assert!(!filter_matches("rust::repair", "rust::repairs::insert"));
        assert!(!filter_matches("rust::repair", "rust"));
        assert!(!filter_matches("rust::repair", "unformatted::rust::repair"));
    }

    #[test]
    fn a_full_name_selects_just_that_test() {
        assert!(filter_matches(
            "rust::repair::insert",
            "rust::repair::insert"
        ));
        assert!(!filter_matches(
            "rust::repair::insert",
            "rust::repair::insert_more"
        ));
    }

    #[test]
    fn a_bare_category_is_a_prefix() {
        assert!(filter_matches("rust", "rust::attributes"));
        assert!(filter_matches("py", "python::layout::replace_nested_block"));
        assert!(!filter_matches("rust", "unformatted::rust::replace"));
        assert!(!filter_matches("", "rust::attributes"));
    }

    fn layered(layers: &[&str]) -> Result<Formatting> {
        let mut formatting = Formatting::default();
        for layer in layers {
            apply_formatting_layer(&mut formatting, &serde_json::from_str(layer)?)?;
        }
        Ok(formatting)
    }

    fn root_formatting_file() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/snapshots")
            .join(FORMATTING_FILE)
    }

    #[test]
    fn no_layers_is_formatting_off() {
        assert_eq!(layered(&[]).unwrap(), Formatting::off());
        assert_eq!(formatting_for(&[]).unwrap(), Formatting::off());
    }

    /// The tree's expectations were written with every built-in formatter on, so
    /// the root turns exactly that set on.
    #[test]
    fn the_root_layer_is_the_built_in_set() {
        assert_eq!(
            formatting_for(&[root_formatting_file()]).unwrap(),
            Formatting::built_in()
        );
    }

    #[test]
    fn star_null_turns_every_formatter_off() {
        assert_eq!(layered(&[r#"{"*": null}"#]).unwrap(), Formatting::off());
    }

    #[test]
    fn a_language_null_turns_just_that_one_off() {
        let root = fs::read_to_string(root_formatting_file()).unwrap();
        assert_eq!(
            layered(&[&root, r#"{"rust": null}"#]).unwrap(),
            Formatting::built_in().without(LanguageName::Rust)
        );
    }

    #[test]
    fn inner_layers_override_outer_ones() {
        assert_eq!(
            layered(&[r#"{"*": null}"#, r#"{"toml": "taplo"}"#]).unwrap(),
            Formatting::off().with(LanguageName::Toml, Formatter::taplo())
        );
        let formatting = layered(&[r#"{"toml": null}"#, r#"{"*": "taplo"}"#]).unwrap();
        for language in LanguageName::value_variants() {
            assert_eq!(
                formatting.formatter(*language),
                Some(&Formatter::taplo()),
                "{language}"
            );
        }
    }

    #[test]
    fn star_applies_before_the_languages_beside_it() {
        assert_eq!(
            layered(&[r#"{"toml": "taplo", "*": null}"#]).unwrap(),
            Formatting::off().with(LanguageName::Toml, Formatter::taplo())
        );
    }

    #[test]
    fn misspellings_are_errors() {
        for layer in [
            r#"{"rsut": null}"#,
            r#"{"rust": "rustformat"}"#,
            r#"{"rust": false}"#,
            r#"{"*": "off"}"#,
            r#"["rust"]"#,
        ] {
            assert!(layered(&[layer]).is_err(), "{layer}");
        }
    }
}
