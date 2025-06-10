use anyhow::{anyhow, Result};
use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};

use semantic_edit_mcp::server::ToolCallParams;
use semantic_edit_mcp::tools::{ExecutionResult, ToolRegistry};

pub struct SnapshotRunner {
    update_mode: bool,
    registry: ToolRegistry,
    test_filter: Option<String>,
}

#[derive(Debug)]
pub struct SnapshotTest {
    pub name: String,
    pub input_path: PathBuf,
    pub args_path: PathBuf,
    pub response_path: PathBuf,
    pub output_path: PathBuf, // NEW: Expected file after transformation
}

#[derive(Debug)]
pub struct SnapshotResult {
    pub test: SnapshotTest,
    pub actual_response: String,
    pub expected_response: Option<String>,
    pub actual_output: Option<String>, // NEW: Actual file content after transformation
    pub expected_output: Option<String>, // NEW: Expected file content
    pub response_matches: bool,
    pub output_matches: bool,
    pub error: Option<String>,
}

#[derive(Debug)]
struct SnapshotExecutionResult {
    response: String,
    output: Option<String>,
}

impl SnapshotRunner {
    pub fn new(update_mode: bool, test_filter: Option<String>) -> Result<Self> {
        let registry = ToolRegistry::new()?;
        Ok(Self {
            update_mode,
            registry,
            test_filter,
        })
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
                    if let Some(input_path) = input_path {
                        let mut output_path = path.join("output");
                        if let Some(extension) = input_path.extension() {
                            output_path.set_extension(extension);
                        }

                        // This is a test directory
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
                        });
                    } else {
                        // Recurse into subdirectories
                        Self::discover_tests_recursive(&path, tests)?;
                    }
                } else {
                    // Recurse into subdirectories
                    Self::discover_tests_recursive(&path, tests)?;
                }
            }
        }

        Ok(())
    }

    /// Run a single snapshot test
    pub async fn run_test(&self, test: SnapshotTest) -> SnapshotResult {
        let result = self.execute_test(&test).await;

        match result {
            Ok(SnapshotExecutionResult { response, output }) if self.update_mode => {
                // Write the actual response as the new expected response
                if let Err(e) = tokio::fs::write(&test.response_path, &response).await {
                    return SnapshotResult {
                        test,
                        actual_response: response,
                        expected_response: None,
                        response_matches: false,
                        output_matches: false,
                        error: Some(format!("Failed to write expected output: {e}")),
                        actual_output: output,
                        expected_output: None,
                    };
                }

                // Write the actual output as the new expected output
                if let Some(output) = &output {
                    if let Err(e) = tokio::fs::write(&test.output_path, &output).await {
                        return SnapshotResult {
                            test,
                            actual_response: response,
                            expected_response: None,
                            response_matches: false,
                            output_matches: false,
                            error: Some(format!("Failed to write expected output: {e}")),
                            actual_output: Some(output.to_string()),
                            expected_output: None,
                        };
                    }
                } else if let Ok(true) = test.output_path.try_exists() {
                    // If there is no expected output but the file exists, delete the file
                    if let Err(e) = tokio::fs::remove_file(&test.output_path).await {
                        return SnapshotResult {
                            test,
                            actual_response: response,
                            expected_response: None,
                            response_matches: false,
                            output_matches: false,
                            error: Some(format!(
                                "No output expected, but was unable to delete: {e}"
                            )),
                            actual_output: None,
                            expected_output: None,
                        };
                    }
                }

                SnapshotResult {
                    test,
                    actual_response: response,
                    expected_response: None,
                    error: None,
                    actual_output: output,
                    expected_output: None,
                    response_matches: true,
                    output_matches: true,
                }
            }

            Ok(SnapshotExecutionResult { response, output }) => {
                // Compare with expected output
                let expected_response = match tokio::fs::read_to_string(&test.response_path).await {
                    Ok(content) => content,
                    Err(_) => {
                        return SnapshotResult {
                            test,
                            actual_response: response,
                            expected_response: None,
                            response_matches: false,
                            output_matches: false,
                            error: Some(
                                "Response file not found. Run with --update to create it."
                                    .to_string(),
                            ),
                            actual_output: output,
                            expected_output: None,
                        };
                    }
                };

                let expected_output = tokio::fs::read_to_string(&test.output_path).await.ok();

                let response_matches = expected_response.trim() == response.trim();
                let output_matches = match (output.as_ref(), expected_output.as_ref()) {
                    (Some(actual), Some(expected)) => actual.trim() == expected.trim(),
                    (None, None) => true,
                    _ => false,
                };

                SnapshotResult {
                    test,
                    actual_response: response,
                    expected_response: Some(expected_response),
                    response_matches,
                    output_matches,
                    error: None,
                    actual_output: output,
                    expected_output,
                }
            }

            Err(e) => SnapshotResult {
                test,
                actual_response: String::new(),
                expected_response: None,
                response_matches: false,
                output_matches: false,
                error: Some(e.to_string()),
                actual_output: None,
                expected_output: None,
            },
        }
    }

    /// Execute a single test and return the tool output
    async fn execute_test(&self, test: &SnapshotTest) -> Result<SnapshotExecutionResult> {
        // Read the arguments
        let args_content = fs::read_to_string(&test.args_path)?;
        let args: Value = serde_json::from_str(&args_content)?;

        // Extract tool name and arguments
        let tool_name = args
            .get("tool")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow!("Test args must specify 'tool' field"))?;

        let mut tool_args = args
            .get("arguments")
            .cloned()
            .unwrap_or(serde_json::json!({}));

        // Update file_path in arguments to point to temp file
        if let Some(args_obj) = tool_args.as_object_mut() {
            args_obj.insert(
                "file_path".to_string(),
                Value::String(test.input_path.to_string_lossy().to_string()),
            );
        }

        // Create tool call params
        let tool_call = ToolCallParams {
            name: tool_name.to_string(),
            arguments: Some(tool_args),
        };

        // Execute the tool - capture both success and error outputs
        match self.registry.execute_tool(&tool_call).await {
            Ok(ExecutionResult::Change {
                response, output, ..
            }) => Ok(SnapshotExecutionResult {
                response,
                output: Some(output),
            }),
            Ok(ExecutionResult::ResponseOnly(response)) => Ok(SnapshotExecutionResult {
                response,
                output: None,
            }),
            Err(e) => {
                // For snapshot testing, we want to capture error messages too
                // Strip any "Tool execution failed: " prefix to get the actual error
                let error_msg = e.to_string();
                if let Some(actual_error) = error_msg.strip_prefix("Tool execution failed: ") {
                    Ok(SnapshotExecutionResult {
                        response: actual_error.to_string(),
                        output: None,
                    })
                } else {
                    Ok(SnapshotExecutionResult {
                        response: error_msg,
                        output: None,
                    })
                }
            }
        }
    }

    /// Run all discovered tests (filtered if TEST_FILTER is set)
    pub async fn run_all_tests(&self) -> Result<Vec<SnapshotResult>> {
        let all_tests = self.discover_tests()?;
        let tests = self.filter_tests(all_tests);

        if let Some(filter) = &self.test_filter {
            println!("🔍 Running filtered tests: {filter}");
            println!("   Found {} matching test(s)", tests.len());
        }

        let mut results = Vec::new();

        for test in tests {
            let result = self.run_test(test).await;
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

        println!("\n📊 Snapshot Test Summary:");
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
                if let Some(error) = &result.error {
                    println!("    Error: {error}");
                } else {
                    if !result.response_matches {
                        println!("    Expected response differs from actual output");
                        println!("    Run with --update to accept changes, or check the diff");
                        if let Some(expected_response) = &result.expected_response {
                            println!("\n    EXPECTED RESPONSE:");
                            println!("    {}", expected_response.replace('\n', "\n    "));
                        }

                        println!("\n    ACTUAL RESPONSE:");
                        println!("    {}", result.actual_response.replace('\n', "\n    "));
                    }

                    if !result.output_matches {
                        println!("    Expected output differs from actual output");
                        println!("    Run with --update to accept changes, or check the diff");
                        if let Some(expected_output) = &result.expected_output {
                            println!("\n    EXPECTED OUTPUT:");
                            println!("    {}", expected_output.replace('\n', "\n    "));
                        } else {
                            println!("\n    EXPECTED OUTPUT: None");
                        }

                        if let Some(actual_output) = &result.actual_output {
                            println!("\n    ACTUAL OUTPUT:");
                            println!("    {}", actual_output.replace('\n', "\n    "));
                        } else {
                            println!("\n    ACTUAL OUTPUT: None");
                        }
                    }
                }

                println!("\n\n");
            }
        }
    }
}
