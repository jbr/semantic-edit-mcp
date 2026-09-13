;; Tree-sitter validation queries for Rust semantic editing
;; Focus on realistic problematic patterns we've actually encountered

;; NOTE: removed the `*.in.function.body` family — struct/enum/union, impl, trait,
;; and mod items declared inside a `fn` body are all valid Rust (local items are a
;; normal pattern, e.g. `#[derive(Deserialize)] struct Response { … }` scoped to
;; the one fn that parses it). Likewise removed `visibility.in.function.body`
;; (`pub` on a local item compiles) and `generic.type.alias.in.function`
;; (`type Alias<T> = …;` in a fn body compiles). Every one of those rules flagged
;; legal code, and because validation runs against the whole result file, a single
;; pre-existing local item rejected *every* edit to the file (see
;; BUG-anchor-inserts-mid-node-and-error-misdiagnoses.md — the real blocker behind
;; all six failed edits was a legal local struct elsewhere in the file).

;; CRITICAL: Methods with self parameters must be inside impl blocks
(source_file
  (function_item
    parameters: (parameters
                 (self_parameter))) @invalid.self.outside.impl)

;; Also catch associated functions that look like methods but aren't in impl
(function_item
 parameters: (parameters
              . (parameter
                 pattern: (identifier) @self_param
                 type: (reference_type
                        (type_identifier)))
              .)
 @invalid.manual.self.outside.impl
 (#eq? @self_param "self")
 (#not-has-ancestor? impl_item))

;; NOTE: removed an `async.in.trait` rule — `async fn` in trait definitions has
;; been stable since Rust 1.75, so the rule flagged valid modern code.

;; NOTE: removed a `function.in.function` rule that flagged a `fn` defined inside
;; another `fn`'s body. Nested/local functions are valid, idiomatic Rust (a private
;; helper scoped to its caller), so the rule produced false positives — and because
;; it runs at result-validation time, it rejected *every* edit to any file that
;; merely contained a nested fn.

;; CRITICAL: Impl blocks cannot be inside other impl blocks
(impl_item
 body: (declaration_list
        (impl_item) @invalid.impl.in.impl))

;; NOTE: removed a `mut.ref.in.const.fn` rule that flagged `&mut` locals in a
;; `const fn` — that has been valid since `const_mut_refs` stabilized (Rust 1.83),
;; so the rule produced false positives on modern code.

;; NOTE: removed a `mut.static.without.unsafe` rule that flagged the *declaration*
;; of a mutable static outside an `unsafe` block. Declaring `static mut` is always
;; legal — only *access* requires `unsafe` — so the rule rejected valid code (and,
;; via prevalidate, blocked editing any file that merely contained one).

;; CRITICAL: Await expressions outside async functions/blocks
((await_expression) @invalid.await.outside.async
 (#not-has-ancestor? function_item)
 (#not-has-ancestor? async_block)
 (#not-has-ancestor? closure_expression))
