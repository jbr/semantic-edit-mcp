use anyhow::{Error, Result};
use diffy::{DiffOptions, PatchFormatter};
use mcplease::traits::Tool;
use semantic_edit_mcp::state::SemanticEditTools;
use semantic_edit_mcp::tools::Tools;
use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};

pub struct SnapshotRunner {
    update_mode: bool,
    state: SemanticEditTools,
    test_filter: Option<String>,
}

#[derive(Debug, Clone)]
pub struct SnapshotTest {
    pub name: String,
    pub base_path: PathBuf,
    pub input_path: Option<PathBuf>,
    pub args_path: PathBuf,
    pub response_path: PathBuf,
    pub output_path: Option<PathBuf>,
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
            if tool["name"] == "stage_operation" {
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
        let state = SemanticEditTools::new(None)?;
        Ok(Self {
            update_mode,
            state,
            test_filter,
        })
    }

    fn reset_state(&mut self, base_path: PathBuf) -> Result<()> {
        self.state = SemanticEditTools::new(None)?;
        self.state.set_default_session_id("test");
        self.state.set_working_directory(base_path, None)?;
        Ok(())
    }

    /// Discover all snapshot tests in the tests/snapshots directory
    pub fn discover_tests(&self) -> Result<Vec<SnapshotTest>> {
        let snapshots_dir = Path::new("tests/snapshots");
        let mut tests = Vec::new();

        Self::discover_tests_recursive(snapshots_dir, &mut tests)?;

        tests.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(tests)
    }

    /// Filter tests based on the TEST_FILTER environment variable
    /// Supports patterns like:
    /// - "basic_operations" (matches all tests starting with this)
    /// - "basic_operations::insert_after_node" (exact match)
    /// - "json_operations,markdown_operations" (multiple patterns separated by commas)
    fn filter_tests(&self, tests: Vec<SnapshotTest>) -> Vec<SnapshotTest> {
        if let Some(filter) = &self.test_filter {
            let patterns: Vec<&str> = filter.split(',').map(|s| s.trim()).collect();

            tests
                .into_iter()
                .filter(|test| {
                    patterns.iter().any(|pattern| {
                        if pattern.is_empty() {
                            false
                        } else if pattern.contains("::") {
                            // Exact match for full test names
                            test.name == *pattern
                        } else {
                            // Prefix match for categories
                            test.name.starts_with(pattern)
                        }
                    })
                })
                .collect()
        } else {
            tests
        }
    }

    fn discover_tests_recursive(dir: &Path, tests: &mut Vec<SnapshotTest>) -> Result<()> {
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
                        .strip_prefix("tests/snapshots")
                        .unwrap_or(&path)
                        .to_string_lossy()
                        .replace('/', "::");

                    tests.push(SnapshotTest {
                        name: test_name,
                        input_path,
                        args_path,
                        response_path,
                        output_path,
                        base_path: path,
                    });
                } else {
                    // Recurse into subdirectories
                    Self::discover_tests_recursive(&path, tests)?;
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
                    error: Some(e.to_string()),
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

    /// Execute a single test and return the tool output
    #[allow(unused_assignments)]
    fn execute_test(&mut self, test: &SnapshotTest) -> Result<SnapshotExecutionResult> {
        self.reset_state(test.base_path.clone())?;

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

        for tool in tool_calls {
            snapshot_execution_result
                .response
                .push_str("=== snapshot test tool call: ");
            snapshot_execution_result.response.push_str(tool.name());
            snapshot_execution_result.response.push_str(" ===\n");
            let (tx, rx) = std::sync::mpsc::channel();
            self.state.set_commit_fn(Some(Box::new(move |_, content| {
                tx.send(content).unwrap();
            })));

            match tool.execute(&mut self.state) {
                Ok(response) => snapshot_execution_result.response.push_str(&response),
                Err(err) => snapshot_execution_result
                    .response
                    .push_str(&err.to_string()),
            }
            snapshot_execution_result.response.push('\n');
            snapshot_execution_result.output = rx.try_recv().ok();
        }
        Ok(snapshot_execution_result)
    }

    /// Run all discovered tests (filtered if TEST_FILTER is set)
    pub fn run_all_tests(&mut self) -> Result<Vec<SnapshotResult>> {
        let all_tests = self.discover_tests()?;
        let tests = self.filter_tests(all_tests);
        assert_ne!(tests.len(), 0);

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
