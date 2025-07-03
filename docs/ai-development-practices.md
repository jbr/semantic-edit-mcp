# Welcome to the Semantic Edit MCP Project

Thanks for collaborating on this semantic code editing tool! This is a quick orientation to help you get up to speed on the project and our working style.

## What We're Building
A semantic code editing tool that uses tree-sitter AST parsing for precise, content-based code modifications. The core innovation is text-anchored node selection that stays stable across edits, unlike fragile line/column positioning.

Key concepts:
- Text-anchored selectors: Edit text by content, not position
- Universal cross-language support via tree-sitter (currently rust, markdown, python, typescript/tsx, javascript/tsx, toml, and json)
- Syntax validation prior to filesystem modification
- Stage-and-commit with the ability to view a diff before writing to disk
- Rich error messages designed for AI agents

## Our Working Style

### Code Philosophy
We're building exclusively for AI agent users, so we optimize for that workflow. A guiding principle is that **the user is never wrong** - it's our job to make the tool support user intentions, not force users to adapt to our design limitations.

We lean heavily on library code to reduce maintenance burden, and we have no meaningful performance constraints - it's always better to do expensive operations in Rust than make AI users simulate them.

### Collaboration Approach
We've found that frequent check-ins work really well. Before making significant changes, a quick "here's what I'm planning to do" helps ensure we're aligned. We're also big believers in questioning assumptions as we learn - often the best path forward only becomes clear once we start moving in some direction.

## Common Patterns We've Learned

### Git Makes Deletion Safe
Since we're always in a git repo, deletion is completely reversible. We've found it's much better to:
- Delete unused code entirely rather than commenting it out
- Remove unnecessary files rather than renaming them to `.bkup`
- Eliminate abstractions that aren't earning their keep

**Example:** If you find a wrapper struct that just contains another struct with no additional behavior, it's usually better to delete the wrapper and use the inner type directly.

### Build Only What We Need Now
We've noticed that placeholder code and "TODO: implement later" stubs tend to become maintenance burden without providing value. Since code generation is so fast, we prefer to:
- Implement features when we actually need them
- Avoid writing methods with `todo!()` for "future extensibility"
- Skip placeholder comments about what we might add later

**Example:** Instead of a service with 5 placeholder methods, write a service with just the 1 method you need today.

### Simplification Over Accumulation
When we see opportunities to simplify, we tend to prefer that over adding layers. Some patterns we've found helpful:
- If two structs hold the same data, consider eliminating one
- If a method is only called from one place, consider inlining it unless it exists to keep functions small and manageable
- If an abstraction's purpose isn't clear, it might not be needed

**Example:** We recently removed an `EditOperation` wrapper that was just holding `(Selector, String)` - using the components directly was cleaner.

### Aggressive Refactoring Is Welcome
Since we have no backwards compatibility constraints (MCP is self-describing), we're free to make structural improvements. Don't hesitate to suggest:
- Renaming types to better reflect their current purpose
- Eliminating historical abstractions that no longer serve a purpose
- Reorganizing module structure

It's strongly encouraged to interrupt tactical implementation conversations with a strategic question like "what are we really trying to accomplish here, and is there a more direct way to get there?"

## Technical Notes
- Use `cargo add` for dependencies rather than editing Cargo.toml directly
- The `rustdoc` tools are available if you need to explore the codebase or understand dependency apis.

## Getting Started
Feel free to ask questions about any of these patterns or if you notice other opportunities for improvement!
