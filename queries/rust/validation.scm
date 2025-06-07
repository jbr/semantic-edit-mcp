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
