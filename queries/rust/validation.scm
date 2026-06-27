;; Tree-sitter validation queries for Rust semantic editing
;; Focus on realistic problematic patterns we've actually encountered

;; CRITICAL: Type definitions cannot be inside function bodies
(function_item 
 body: (block
        [(struct_item) (enum_item) (union_item)] @invalid.type.in.function.body))

;; CRITICAL: Impl blocks cannot be inside function bodies  
(function_item
 body: (block
        (impl_item) @invalid.impl.in.function.body))

;; CRITICAL: Trait definitions cannot be inside function bodies
(function_item
 body: (block
        (trait_item) @invalid.trait.in.function.body))

;; Module declarations inside function bodies are invalid
(function_item
 body: (block
        (mod_item) @invalid.mod.in.function.body))


(function_item
 body: (block
        (trait_item) @invalid.trait.in.function.body))



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





;; CRITICAL: Async functions cannot be inside trait definitions (unless async trait)
(trait_item
 body: (declaration_list
        (function_item
         (function_modifiers
          "async")) @invalid.async.in.trait))

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

;; CRITICAL: Visibility modifiers on items inside functions
(function_item
 body: (block
        [(struct_item (visibility_modifier))
         (enum_item (visibility_modifier))
         (function_item (visibility_modifier))
         (const_item (visibility_modifier))
         (static_item (visibility_modifier))] @invalid.visibility.in.function.body))


;; Type aliases with generics inside function bodies might be questionable
(function_item
  body: (block
    (type_item
      type_parameters: (type_parameters)) @invalid.generic.type.alias.in.function))

;; NOTE: removed a `mut.static.without.unsafe` rule that flagged the *declaration*
;; of a mutable static outside an `unsafe` block. Declaring `static mut` is always
;; legal — only *access* requires `unsafe` — so the rule rejected valid code (and,
;; via prevalidate, blocked editing any file that merely contained one).

;; CRITICAL: Await expressions outside async functions/blocks
((await_expression) @invalid.await.outside.async
 (#not-has-ancestor? function_item)
 (#not-has-ancestor? async_block)
 (#not-has-ancestor? closure_expression))


