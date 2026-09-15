//! Minimal XML text escaping shared by every model-facing renderer (context
//! blocks, prioritization render). Task titles and user prose are the inputs;
//! the five predefined entities cover well-formedness.

pub fn escape_xml(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for ch in input.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            other => out.push(other),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escapes_the_five_predefined_entities() {
        assert_eq!(
            escape_xml(r#"a < b & "c" 'd'"#),
            "a &lt; b &amp; &quot;c&quot; &apos;d&apos;"
        );
    }

    #[test]
    fn plain_text_passes_through() {
        assert_eq!(escape_xml("普通文本 123"), "普通文本 123");
    }
}
