# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- **Multi-file support**: `open_file` tool now accepts `file_paths` array for opening multiple files in a single operation
- **Content-based versioning**: Stable, deterministic identifiers based on file content hash instead of random strings
- **Array-only interface**: Simplified `open_file` API using consistent `file_paths` parameter for both single and multiple files
- **Enhanced language detection**: Per-file language detection and AST parsing in multi-file operations
- **Diff validation**: Helpful error messages when `diff_since` is used with multiple files
- **CI-compatible testing**: Test runner supports relative paths for GitHub Actions compatibility

### Changed

- **BREAKING**: `open_file` tool now requires `file_paths` array instead of `file_path` string
- **BREAKING**: Tool response format includes clear separators between multiple files
- Hash-based file versioning replaces random identifiers for consistent testing
- Language hints now apply to all files in a multi-file operation

### Fixed

- Test runner correctly handles relative file paths for CI environments
- Stable content hashing eliminates random test failures
## [0.1.2](https://github.com/jbr/semantic-edit-mcp/compare/v0.1.1...v0.1.2) - 2025-06-07

### Added

- *(validation)* implement tree-sitter native context validation system
- add specialized insertion tools and enhanced error messages
- implement preview-only mode for safe operation testing

### Fixed

- only build on nightly in ci, resolve clippy lints
- clippy and fmt

### Other

- remove examples from git
- update PROJECT_SUMMARY.md to reflect Phase 1 completion

## [0.1.1](https://github.com/jbr/semantic-edit-mcp/compare/v0.1.0...v0.1.1) - 2025-06-06

### Fixed

- add Cargo.lock to repository
- LICENSE
- add lifetimes to find_node_by_position
- build on stable

### Other

- clippy
- add .github
- clippy and fmt
