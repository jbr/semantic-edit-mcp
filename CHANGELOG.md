# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.3](https://github.com/jbr/semantic-edit-mcp/compare/v0.1.2...v0.1.3) - 2025-06-08

### Added

- *(validation)* add complete context validation to insert_before_node and wrap_node
- *(validation)* make context validation language-aware
- *(validation)* add syntax validation safety check to prevent file corruption
- *(integration)* wire up new language system with existing tools
- *(languages)* implement Phase 1 - query-based language abstractions

### Fixed

- *(lint)* clippy
- *(lint)* fmt

### Other

- comprehensive documentation update to reflect current state
- add adding-languages.md

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
