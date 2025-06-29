;; JSON operations queries for semantic editing

;; Find JSON objects for property insertion
(object) @insertable_object

;; Find JSON properties by key name  
(pair 
  key: (string) @key 
  value: (_) @value
  (#eq? @key "\"TARGET_KEY\"")) @property

;; Find all properties in an object
(object
  (pair
    key: (string) @all_keys))

;; Find JSON arrays for item insertion
(array) @insertable_array

;; Find array items
(array
  (_) @array_item)

;; Find values for replacement
(pair 
  key: (string) @key 
  value: (_) @replaceable_value
  (#eq? @key "\"TARGET_KEY\""))

;; Find nested objects by path
(object
  (pair
    key: (string) @parent_key
    value: (object) @nested_object
    (#eq? @parent_key "\"TARGET_PARENT\"")))

