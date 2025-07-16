;; Tree-sitter validation queries for Python semantic editing
;; Conservative rules to avoid false positives

;; CRITICAL: Class definitions cannot be inside function bodies
(function_definition 
 body: (block
        (class_definition) @invalid.class.in.function.body))

;; CRITICAL: return_statement at module level  
(module
  (return_statement) @invalid.return.at.module.level)


;; Only flag functions with 'self' that are DIRECTLY at module level
(module
  (function_definition
    parameters: (parameters
                 (identifier) @self_param
                 (#eq? @self_param "self")))
  @invalid.self.method.at.module.level)

(module
 (return_statement) @invalid.return.at.module.level)

(module
  (expression_statement
    (yield) @invalid.yield.at.module.level))

