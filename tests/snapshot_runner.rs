use anyhow::{anyhow, Result};
use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};

use semantic_edit_mcp::tools::ToolRegistry;
use semantic_edit_mcp::server::ToolCallParams;

pub struct SnapshotRunner {
    update_mode: bool,
    registry: ToolRegistry,
}

#[derive(Debug)]
pub struct SnapshotTest {
    pub name: String,
    pub input_file: PathBuf,
    pub args_file: PathBuf,
    pub expected_output_file: PathBuf,
}

#[derive(Debug)]
pub struct SnapshotResult {
    pub test: SnapshotTest,
    pub actual_output: String,
    pub expected_output: Option<String>,
    pub passed: bool,
    pub error: Option<String>,
}

impl SnapshotRunner {
    pub fn new(update_mode: bool) -> Result<Self> {
        let registry = ToolRegistry::new()?;
        Ok(Self {
            update_mode,
            registry,
        })
    }

    /// Discover all snapshot tests in the tests/snapshots directory
    pub fn discover_tests(&self) -> Result<Vec<SnapshotTest>> {
        let snapshots_dir = Path::new("tests/snapshots");
        let mut tests = Vec::new();

        self.discover_tests_recursive(snapshots_dir, &mut tests)?;
        
        tests.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(tests)
    }

    fn discover_tests_recursive(&self, dir: &Path, tests: &mut Vec<SnapshotTest>) -> Result<()> {
        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();

            if path.is_dir() {
                // Check if this directory contains a complete test (input + args files)
                let args_file = path.join("args.json");
                let expected_output_file = path.join("expected_output.txt");

                let input_file = if path.join("input.rs").exists() {
                    Some(path.join("input.rs"))
                } else if path.join("input.json").exists() {
                    Some(path.join("input.json"))
                } else {
                    None
                };

                if args_file.exists() && input_file.is_some() {
                    // This is a test directory
                    let test_name = path.strip_prefix("tests/snapshots")
                        .unwrap_or(&path)
                        .to_string_lossy()
                        .replace('/', "::");

                    tests.push(SnapshotTest {
                        name: test_name,
                        input_file: input_file.unwrap(),
                        args_file,
                        expected_output_file,
                    });
                } else {
                    // Recurse into subdirectories
                    self.discover_tests_recursive(&path, tests)?;
                }
            }
        }

        Ok(())
    }

    /// Run a single snapshot test
    pub async fn run_test(&self, test: SnapshotTest) -> SnapshotResult {
        let result = self.execute_test(&test).await;

        match result {
            Ok(actual_output) => {
                if self.update_mode {
                    // Write the actual output as the new expected output
                    if let Err(e) = fs::write(&test.expected_output_file, &actual_output) {
                        return SnapshotResult {
                            test,
                            actual_output,
                            expected_output: None,
                            passed: false,
                            error: Some(format!("Failed to write expected output: {}", e)),
                        };
                    }

                    SnapshotResult {
                        test,
                        actual_output,
                        expected_output: None,
                        passed: true,
                        error: None,
                    }
                } else {
                    // Compare with expected output
                    let expected_output = match fs::read_to_string(&test.expected_output_file) {
                        Ok(content) => Some(content),
                        Err(_) => {
                            return SnapshotResult {
                                test,
                                actual_output,
                                expected_output: None,
                                passed: false,
                                error: Some("Expected output file not found. Run with --update to create it.".to_string()),
                            };
                        }
                    };

                    let passed = expected_output.as_ref().map_or(false, |expected| expected.trim() == actual_output.trim());

                    SnapshotResult {
                        test,
                        actual_output,
                        expected_output,
                        passed,
                        error: None,
                    }
                }
            }
            Err(e) => SnapshotResult {
                test,
                actual_output: String::new(),
                expected_output: None,
                passed: false,
                error: Some(e.to_string()),
            },
        }
    }

    /// Execute a single test and return the tool output
        /// Execute a single test and return the tool output
    async fn execute_test(&self, test: &SnapshotTest) -> Result<String> {
        // Read the arguments
        let args_content = fs::read_to_string(&test.args_file)?;
        let args: Value = serde_json::from_str(&args_content)?;

        // Extract tool name and arguments
        let tool_name = args.get("tool")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow!("Test args must specify 'tool' field"))?;

        let tool_args = args.get("arguments")
            .cloned()
            .unwrap_or(serde_json::json!({}));

        // If the test has an input file, copy it to a temporary location
        // and update the file_path in arguments
        let mut final_args = tool_args;
        if test.input_file.exists() {
            let temp_file = self.create_temp_input_file(test)?;
            
            // Update file_path in arguments to point to temp file
            if let Some(args_obj) = final_args.as_object_mut() {
                args_obj.insert("file_path".to_string(), Value::String(temp_file.to_string_lossy().to_string()));
            }
        }

        // Create tool call params
        let tool_call = ToolCallParams {
            name: tool_name.to_string(),
            arguments: Some(final_args),
        };

        // Execute the tool - capture both success and error outputs
        match self.registry.execute_tool(&tool_call).await {
            Ok(output) => Ok(output),
            Err(e) => {
                // For snapshot testing, we want to capture error messages too
                // Strip any "Tool execution failed: " prefix to get the actual error
                let error_msg = e.to_string();
                if let Some(actual_error) = error_msg.strip_prefix("Tool execution failed: ") {
                    Ok(actual_error.to_string())
                } else {
                    Ok(error_msg)
                }
            }
        }
    }

    fn create_temp_input_file(&self, test: &SnapshotTest) -> Result<PathBuf> {
        let input_content = fs::read_to_string(&test.input_file)?;
        
        // Create a temporary file in the project directory (so relative paths work)
        let temp_file = Path::new("tests").join(format!("temp_input_{}.{}", 
            test.name.replace("::", "_"),
            test.input_file.extension().unwrap_or_default().to_string_lossy()
        ));

        fs::write(&temp_file, input_content)?;
        Ok(temp_file)
    }

    /// Run all discovered tests
    pub async fn run_all_tests(&self) -> Result<Vec<SnapshotResult>> {
        let tests = self.discover_tests()?;
        let mut results = Vec::new();

        for test in tests {
            let result = self.run_test(test).await;
            results.push(result);
        }

        // Clean up temp files
        self.cleanup_temp_files()?;

        Ok(results)
    }

    fn cleanup_temp_files(&self) -> Result<()> {
        let test_dir = Path::new("tests");
        for entry in fs::read_dir(test_dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.file_name()
                .and_then(|name| name.to_str())
                .map_or(false, |name| name.starts_with("temp_input_"))
            {
                let _ = fs::remove_file(path); // Ignore errors
            }
        }
        Ok(())
    }

    /// Print a summary of test results
        /// Print a summary of test results
    pub fn print_summary(&self, results: &[SnapshotResult]) {
        let total = results.len();
        let passed = results.iter().filter(|r| r.passed).count();
        let failed = total - passed;

        println!("\n📊 Snapshot Test Summary:");
        println!("  Total:  {}", total);
        println!("  Passed: {}", passed);
        println!("  Failed: {}", failed);

        if self.update_mode {
            println!("  Mode:   UPDATE (expected outputs written)");
        } else {
            println!("  Mode:   VERIFY");
        }

        if failed > 0 {
            println!("\n❌ Failed tests:");
            for result in results.iter().filter(|r| !r.passed) {
                println!("  • {}", result.test.name);
                if let Some(error) = &result.error {
                    println!("    Error: {}", error);
                } else if let Some(_expected) = &result.expected_output {
                    println!("    Expected output differs from actual output");
                    println!("    Run with --update to accept changes, or check the diff");
                    
                    // Print actual output for debugging
                    println!("\n    ACTUAL OUTPUT:");
                    println!("    {}", result.actual_output.replace('\n', "\n    "));
                }
            }
        }

        if passed > 0 {
            println!("\n✅ Passed tests:");
            for result in results.iter().filter(|r| r.passed) {
                println!("  • {}", result.test.name);
            }
        }
    }
}
