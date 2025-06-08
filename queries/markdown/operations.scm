;; Markdown operations queries for semantic editing (tree-sitter-md)

;; Find sections for structural operations
(section) @insertable_section

;; Find headings by level for insertion operations
(atx_heading 
  (atx_h1_marker) 
  (inline) @h1_content) @insertable_h1

(atx_heading 
  (atx_h2_marker) 
  (inline) @h2_content) @insertable_h2

(atx_heading 
  (atx_h3_marker) 
  (inline) @h3_content) @insertable_h3

(atx_heading 
  (atx_h4_marker) 
  (inline) @h4_content) @insertable_h4

(atx_heading 
  (atx_h5_marker) 
  (inline) @h5_content) @insertable_h5

(atx_heading 
  (atx_h6_marker) 
  (inline) @h6_content) @insertable_h6

;; Find all headings regardless of level
(atx_heading) @insertable_heading

;; Find paragraphs for content insertion
(paragraph) @insertable_paragraph

;; Find lists for item addition
(list) @insertable_list

;; Find list items for insertion
(list_item) @insertable_list_item

;; Find code blocks for insertion/replacement
(fenced_code_block
  (info_string (language) @language)?
  (code_fence_content) @code_content) @insertable_code_block

;; Find block quotes
(block_quote) @insertable_block_quote

;; Find inline content for modification
(inline) @replaceable_inline

;; Find document root for top-level insertion
(document) @insertable_document

;; Specific patterns for finding headings by content text
;; Note: tree-sitter-md structure makes text matching more complex
;; We'll need to match on the inline content

;; Find code blocks by language
(fenced_code_block
  (info_string 
    (language) @lang
    (#eq? @lang "TARGET_LANG"))) @target_code_block

;; Find sections containing specific headings
(section
  (atx_heading
    (inline) @heading_text
    (#eq? @heading_text "TARGET_HEADING"))) @target_section
