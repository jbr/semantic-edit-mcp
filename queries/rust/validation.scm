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

;; Use declarations inside function bodies (should be at module level)
(function_item
 body: (block
        (use_declaration) @invalid.use.in.function.body))

;; Static/const items inside function bodies
(function_item
 body: (block
        [(const_item) (static_item)] @invalid.const.in.function.body))


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

;; CRITICAL: Functions cannot be defined inside other functions (nested functions)
(function_item
 body: (block
        (function_item) @invalid.function.in.function))

;; CRITICAL: Impl blocks cannot be inside other impl blocks
(impl_item
 body: (declaration_list
        (impl_item) @invalid.impl.in.impl))

;; CRITICAL: Const functions with non-const operations in const contexts
;; This catches some obvious cases like mutable references in const fn
(function_item
 (function_modifiers
  "const")
 body: (block
        (let_declaration
         pattern: (_)
         type: (reference_type
                (mutable_specifier)) @invalid.mut.ref.in.const.fn)))



;; CRITICAL: Break/continue outside of loops

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

;; CRITICAL: Mutable static items without unsafe
(static_item
 (mutable_specifier)
 value: (_) @invalid.mut.static.without.unsafe
 (#not-has-ancestor? unsafe_block))


;; CRITICAL: Await expressions outside async functions/blocks
((await_expression) @invalid.await.outside.async
 (#not-has-ancestor? function_item)
 (#not-has-ancestor? async_block)
 (#not-has-ancestor? closure_expression))


