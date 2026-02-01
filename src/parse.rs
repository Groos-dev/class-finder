use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParsedClass {
    pub class_name: String,
    pub content: String,
    pub content_hash: String,
}

pub fn parse_decompiled_output(content: &str) -> Vec<ParsedClass> {
    let normalized = content.replace("\r\n", "\n");
    let marker = "/*\n * Decompiled with CFR";

    let mut starts: Vec<usize> = normalized.match_indices(marker).map(|(i, _)| i).collect();
    if starts.is_empty() {
        if let Some(name) = extract_class_name(&normalized) {
            let content_hash = hash_content(&normalized);
            return vec![ParsedClass {
                class_name: name,
                content: normalized,
                content_hash,
            }];
        }
        return Vec::new();
    }

    starts.sort_unstable();

    let mut results = Vec::new();
    for (idx, start) in starts.iter().enumerate() {
        let end = starts.get(idx + 1).copied().unwrap_or(normalized.len());
        let class_content = normalized[*start..end].trim().to_string();
        if class_content.is_empty() {
            continue;
        }

        if let Some(class_name) = extract_class_name(&class_content) {
            let content_hash = hash_content(&class_content);
            results.push(ParsedClass {
                class_name,
                content: class_content,
                content_hash,
            });
        }
    }

    results
}

pub fn extract_class_name(content: &str) -> Option<String> {
    let mut package: Option<String> = None;
    let mut type_name: Option<String> = None;

    for line in content.lines() {
        let line = line.trim();

        if package.is_none() && line.starts_with("package ") {
            let pkg = line
                .trim_start_matches("package ")
                .trim_end_matches(';')
                .trim()
                .to_string();
            if !pkg.is_empty() {
                package = Some(pkg);
            }
        }

        if type_name.is_none()
            && let Some(name) = extract_type_name_from_line(line)
        {
            type_name = Some(name);
        }

        if package.is_some() && type_name.is_some() {
            break;
        }
    }

    let type_name = type_name?;
    Some(match package {
        Some(pkg) => format!("{pkg}.{type_name}"),
        None => type_name,
    })
}

fn extract_type_name_from_line(line: &str) -> Option<String> {
    let keywords = ["class ", "interface ", "enum ", "record ", "@interface "];

    for kw in keywords {
        if let Some(pos) = line.find(kw) {
            let after = &line[pos + kw.len()..];
            let token = after.split_whitespace().next()?;
            let token = token.trim_end_matches('{').trim();
            let token = token.split('<').next().unwrap_or(token);
            if !token.is_empty() {
                return Some(token.to_string());
            }
        }
    }

    None
}

pub fn hash_content(content: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(content.as_bytes());
    let digest = hasher.finalize();
    hex::encode(digest)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_decompiled_output_splits_multiple_classes() {
        let input = r#"/*
 * Decompiled with CFR 0.152.
 */
package org.apache.commons.lang3;

public class StringUtils {
}
/*
 * Decompiled with CFR 0.152.
 */
package org.apache.commons.lang3;

public class ArrayUtils {
}
"#;

        let parsed = parse_decompiled_output(input);
        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed[0].class_name, "org.apache.commons.lang3.StringUtils");
        assert_eq!(parsed[1].class_name, "org.apache.commons.lang3.ArrayUtils");
    }

    #[test]
    fn extract_class_name_handles_generics() {
        let input = r#"
package a.b;
public final class Foo<T> extends Bar {
}
"#;
        assert_eq!(extract_class_name(input).as_deref(), Some("a.b.Foo"));
    }
}
