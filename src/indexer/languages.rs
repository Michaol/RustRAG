use tree_sitter::Language;

pub struct LanguageConfig {
    pub name: &'static str,
    pub language: Language,
    pub extensions: &'static [&'static str],
    pub query: &'static str,
    pub call_query: &'static str,
    pub import_query: &'static str,
    pub inherit_query: &'static str,
}

impl LanguageConfig {
    pub fn get_all() -> Vec<LanguageConfig> {
        vec![
            go_config(),
            python_config(),
            typescript_config(),
            javascript_config(),
            rust_config(),
        ]
    }

    pub fn get_by_extension(ext: &str) -> Option<LanguageConfig> {
        Self::get_all()
            .into_iter()
            .find(|c| c.extensions.contains(&ext))
    }

    pub fn get_by_name(name: &str) -> Option<LanguageConfig> {
        Self::get_all().into_iter().find(|c| c.name == name)
    }
}

fn go_config() -> LanguageConfig {
    LanguageConfig {
        name: "go",
        language: tree_sitter_go::LANGUAGE.into(),
        extensions: &["go"],
        query: r#"
(function_declaration
  name: (identifier) @name) @function

(method_declaration
  name: (field_identifier) @name) @method

(type_declaration
  (type_spec
    name: (type_identifier) @name
    type: (struct_type))) @struct

(type_declaration
  (type_spec
    name: (type_identifier) @name
    type: (interface_type))) @interface
"#,
        call_query: r#"
(call_expression
  function: (identifier) @call)
(call_expression
  function: (selector_expression
    field: (field_identifier) @call))
"#,
        import_query: r#"
(import_spec
  path: (interpreted_string_literal) @import)
"#,
        inherit_query: "",
    }
}

fn python_config() -> LanguageConfig {
    LanguageConfig {
        name: "python",
        language: tree_sitter_python::LANGUAGE.into(),
        extensions: &["py"],
        query: r#"
(function_definition
  name: (identifier) @name) @function

(class_definition
  name: (identifier) @name) @class
"#,
        call_query: r#"
(call
  function: (identifier) @call)
(call
  function: (attribute
    attribute: (identifier) @call))
"#,
        import_query: r#"
(import_statement
  name: (dotted_name) @import)
(import_from_statement
  module_name: (dotted_name) @import)
"#,
        inherit_query: r#"
(class_definition
  superclasses: (argument_list
    (identifier) @inherit))
"#,
    }
}

fn typescript_config() -> LanguageConfig {
    LanguageConfig {
        name: "typescript",
        language: tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
        extensions: &["ts", "tsx"],
        query: r#"
(function_declaration
  name: (identifier) @name) @function

(lexical_declaration
  (variable_declarator
    name: (identifier) @name
    value: (arrow_function))) @function

(class_declaration
  name: (type_identifier) @name) @class

(interface_declaration
  name: (type_identifier) @name) @interface

(method_definition
  name: (property_identifier) @name) @method
"#,
        call_query: r#"
(call_expression
  function: (identifier) @call)
(call_expression
  function: (member_expression
    property: (property_identifier) @call))
"#,
        import_query: r#"
(import_statement
  source: (string) @import)
"#,
        inherit_query: r#"
(class_declaration
  (class_heritage
    (extends_clause
      value: (identifier) @inherit)))
(class_declaration
  (class_heritage
    (implements_clause
      (type_identifier) @inherit)))
"#,
    }
}

fn javascript_config() -> LanguageConfig {
    LanguageConfig {
        name: "javascript",
        language: tree_sitter_javascript::LANGUAGE.into(),
        extensions: &["js", "jsx"],
        query: r#"
(function_declaration
  name: (identifier) @name) @function

(lexical_declaration
  (variable_declarator
    name: (identifier) @name
    value: (arrow_function))) @function

(class_declaration
  name: (identifier) @name) @class

(method_definition
  name: (property_identifier) @name) @method
"#,
        call_query: r#"
(call_expression
  function: (identifier) @call)
(call_expression
  function: (member_expression
    property: (property_identifier) @call))
"#,
        import_query: r#"
(import_statement
  source: (string) @import)
"#,
        inherit_query: r#"
(class_declaration
  (class_heritage
    (identifier) @inherit))
"#,
    }
}

fn rust_config() -> LanguageConfig {
    LanguageConfig {
        name: "rust",
        language: tree_sitter_rust::LANGUAGE.into(),
        extensions: &["rs"],
        query: r#"
(function_item
  name: (identifier) @name) @function

(impl_item
  trait: (type_identifier)? @trait
  type: (type_identifier) @name) @struct

(struct_item
  name: (type_identifier) @name) @struct

(enum_item
  name: (type_identifier) @name) @struct

(trait_item
  name: (type_identifier) @name) @interface

(mod_item
  name: (identifier) @name) @function
"#,
        call_query: r#"
(call_expression
  function: (identifier) @call)
(call_expression
  function: (field_expression
    field: (field_identifier) @call))
(call_expression
  function: (scoped_identifier
    name: (identifier) @call))
"#,
        import_query: r#"
(use_declaration
  argument: (scoped_identifier) @import)
(use_declaration
  argument: (identifier) @import)
(use_declaration
  argument: (use_wildcard) @import)
"#,
        inherit_query: "",
    }
}
