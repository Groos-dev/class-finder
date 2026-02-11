use serde::Serialize;
use tree_sitter::Parser;

#[derive(Debug, Clone, Serialize)]
pub struct ClassStructure {
    pub package: String,
    pub imports: Vec<String>,
    pub class_declaration: String,
    pub fields: Vec<String>,
    pub methods: Vec<String>,
}

pub fn parse_class_structure(source: &str) -> Option<ClassStructure> {
    if source.trim().is_empty() {
        return None;
    }

    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_java::LANGUAGE.into())
        .ok()?;
    let tree = parser.parse(source, None)?;
    let root = tree.root_node();
    let bytes = source.as_bytes();

    let mut package = String::new();
    let mut imports = Vec::new();
    let mut class_declaration = String::new();
    let mut fields = Vec::new();
    let mut methods = Vec::new();

    let mut cursor = root.walk();
    for child in root.children(&mut cursor) {
        match child.kind() {
            "package_declaration" => {
                package = extract_package(&child, bytes);
            }
            "import_declaration" => {
                if let Some(imp) = extract_import(&child, bytes) {
                    imports.push(imp);
                }
            }
            "class_declaration"
            | "interface_declaration"
            | "enum_declaration"
            | "record_declaration"
            | "annotation_type_declaration" => {
                class_declaration = extract_class_declaration(&child, bytes);
                extract_members(&child, bytes, &mut fields, &mut methods);
            }
            _ => {}
        }
    }

    Some(ClassStructure {
        package,
        imports,
        class_declaration,
        fields,
        methods,
    })
}

fn extract_package(node: &tree_sitter::Node, source: &[u8]) -> String {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "scoped_identifier" || child.kind() == "identifier" {
            return node_text(&child, source).to_string();
        }
    }
    String::new()
}

fn extract_import(node: &tree_sitter::Node, source: &[u8]) -> Option<String> {
    let mut path = String::new();

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        match child.kind() {
            "static" => {}
            "scoped_identifier" | "identifier" | "asterisk" => {
                path = node_text(&child, source).to_string();
            }
            _ => {}
        }
    }

    if path.is_empty() { None } else { Some(path) }
}

fn extract_class_declaration(node: &tree_sitter::Node, source: &[u8]) -> String {
    let mut result = String::new();

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        match child.kind() {
            "class_body"
            | "interface_body"
            | "enum_body"
            | "annotation_type_body"
            | "record_declaration_body" => break,
            _ => {
                let text = node_text(&child, source);
                if !result.is_empty() && !needs_no_leading_space(child.kind()) {
                    result.push(' ');
                }
                result.push_str(text);
            }
        }
    }

    result.trim().to_string()
}

fn extract_members(
    node: &tree_sitter::Node,
    source: &[u8],
    fields: &mut Vec<String>,
    methods: &mut Vec<String>,
) {
    let body = find_body(node);
    let body = match body {
        Some(b) => b,
        None => return,
    };

    let mut cursor = body.walk();
    for child in body.children(&mut cursor) {
        match child.kind() {
            "field_declaration" => {
                fields.push(normalize_whitespace(node_text(&child, source)));
            }
            "method_declaration" | "constructor_declaration" => {
                if let Some(sig) = extract_method_signature(&child, source) {
                    methods.push(sig);
                }
            }
            "annotation_type_element_declaration" => {
                let text = normalize_whitespace(node_text(&child, source));
                methods.push(text);
            }
            "constant_declaration" => {
                fields.push(normalize_whitespace(node_text(&child, source)));
            }
            "enum_constant" => {
                fields.push(normalize_whitespace(node_text(&child, source)));
            }
            "enum_body_declarations" => {
                let mut inner_cursor = child.walk();
                for inner in child.children(&mut inner_cursor) {
                    match inner.kind() {
                        "field_declaration" => {
                            fields.push(normalize_whitespace(node_text(&inner, source)));
                        }
                        "method_declaration" | "constructor_declaration" => {
                            if let Some(sig) = extract_method_signature(&inner, source) {
                                methods.push(sig);
                            }
                        }
                        _ => {}
                    }
                }
            }
            "record_declaration_body" => {}
            _ => {}
        }
    }
}

fn find_body<'a>(node: &tree_sitter::Node<'a>) -> Option<tree_sitter::Node<'a>> {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        match child.kind() {
            "class_body" | "interface_body" | "enum_body" | "annotation_type_body" => {
                return Some(child);
            }
            _ => {}
        }
    }
    None
}

fn extract_method_signature(node: &tree_sitter::Node, source: &[u8]) -> Option<String> {
    let mut result = String::new();

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        match child.kind() {
            "block" | "constructor_body" => break,
            ";" => continue,
            _ => {
                let text = node_text(&child, source);
                if !result.is_empty() && !needs_no_leading_space(child.kind()) {
                    result.push(' ');
                }
                result.push_str(text);
            }
        }
    }

    let sig = result.trim().to_string();
    if sig.is_empty() { None } else { Some(sig) }
}

fn node_text<'a>(node: &tree_sitter::Node, source: &'a [u8]) -> &'a str {
    node.utf8_text(source).unwrap_or("")
}

fn normalize_whitespace(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn needs_no_leading_space(kind: &str) -> bool {
    matches!(
        kind,
        "type_parameters" | "formal_parameters" | "type_arguments"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_simple_class() {
        let source = r#"
package org.example;

import java.util.List;
import java.util.Map;

public class Foo extends Bar implements Baz {
    private String name;
    private int count;

    public Foo(String name) {
        this.name = name;
    }

    public String getName() {
        return name;
    }

    public void setName(String name) {
        this.name = name;
    }
}
"#;
        let result = parse_class_structure(source).unwrap();
        assert_eq!(result.package, "org.example");
        assert_eq!(result.imports, vec!["java.util.List", "java.util.Map"]);
        assert!(
            result
                .class_declaration
                .contains("public class Foo extends Bar implements Baz")
        );
        assert_eq!(result.fields.len(), 2);
        assert!(result.fields[0].contains("private String name"));
        assert!(result.fields[1].contains("private int count"));
        assert_eq!(result.methods.len(), 3);
        assert!(result.methods[0].contains("public Foo(String name)"));
        assert!(result.methods[1].contains("public String getName()"));
        assert!(result.methods[2].contains("public void setName(String name)"));
    }

    #[test]
    fn parse_interface() {
        let source = r#"
package org.example;

public interface Service<T> {
    T find(String id);
    void save(T entity);
}
"#;
        let result = parse_class_structure(source).unwrap();
        assert_eq!(result.package, "org.example");
        assert!(
            result
                .class_declaration
                .contains("public interface Service<T>")
        );
        assert_eq!(result.methods.len(), 2);
        assert!(result.methods[0].contains("T find(String id)"));
        assert!(result.methods[1].contains("void save(T entity)"));
    }

    #[test]
    fn parse_enum() {
        let source = r#"
package org.example;

public enum Color {
    RED,
    GREEN,
    BLUE;

    private int value;

    public int getValue() {
        return value;
    }
}
"#;
        let result = parse_class_structure(source).unwrap();
        assert!(result.class_declaration.contains("public enum Color"));
        assert!(result.fields.iter().any(|f| f.contains("RED")));
        assert!(
            result
                .fields
                .iter()
                .any(|f| f.contains("private int value"))
        );
        assert!(result.methods[0].contains("public int getValue()"));
    }

    #[test]
    fn parse_annotation_type() {
        let source = r#"
package org.springframework.stereotype;

import java.lang.annotation.Documented;
import java.lang.annotation.ElementType;
import java.lang.annotation.Retention;
import java.lang.annotation.RetentionPolicy;
import java.lang.annotation.Target;

@Target({ElementType.TYPE})
@Retention(RetentionPolicy.RUNTIME)
@Documented
public @interface Component {
    String value() default "";
}
"#;
        let result = parse_class_structure(source).unwrap();
        assert_eq!(result.package, "org.springframework.stereotype");
        assert_eq!(result.imports.len(), 5);
        assert!(result.class_declaration.contains("@interface Component"));
        assert!(result.methods.iter().any(|m| m.contains("value()")));
    }

    #[test]
    fn parse_class_with_annotations_and_generics() {
        let source = r#"
package org.example;

import java.util.List;

public abstract class AbstractRepository<T, ID> implements Repository<T, ID> {
    @Autowired
    private EntityManager em;

    @Override
    public T findById(ID id) {
        return em.find(getEntityClass(), id);
    }

    protected abstract Class<T> getEntityClass();
}
"#;
        let result = parse_class_structure(source).unwrap();
        assert!(
            result
                .class_declaration
                .contains("public abstract class AbstractRepository<T, ID>")
        );
        assert!(result.fields.iter().any(|f| f.contains("EntityManager em")));
        assert_eq!(result.methods.len(), 2);
    }

    #[test]
    fn parse_static_import() {
        let source = r#"
package org.example;

import static org.junit.Assert.assertEquals;

public class Test {
}
"#;
        let result = parse_class_structure(source).unwrap();
        assert_eq!(result.imports, vec!["org.junit.Assert.assertEquals"]);
    }

    #[test]
    fn parse_empty_source_returns_none() {
        assert!(parse_class_structure("").is_none());
    }
}
