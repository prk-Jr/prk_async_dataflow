/// Extracts the first JSON block (balanced braces or brackets) from a text string.
/// It supports both JSON objects (starting with `{`) and JSON arrays (starting with `[`).
/// This function ignores braces/brackets that occur inside string literals.
///
/// Returns a tuple of the extracted JSON string and the number of bytes consumed.
pub fn extract_json(text: &str) -> Option<(String, usize)> {
    let text = text.trim_start();
    let start = text.find(|c: char| c == '{' || c == '[')?;
    let text_slice = &text[start..];
    let first_char = text_slice.chars().next()?;
    let (opening, closing) = match first_char {
        '{' => ('{', '}'),
        '[' => ('[', ']'),
        _ => return None,
    };

    let mut count = 0;
    let mut in_string = false;
    let mut escape = false;
    let mut end = None;
    for (i, c) in text_slice.char_indices() {
        if in_string {
            if escape {
                escape = false;
            } else if c == '\\' {
                escape = true;
            } else if c == '"' {
                in_string = false;
            }
        } else {
            if c == '"' {
                in_string = true;
            } else if c == opening {
                count += 1;
            } else if c == closing {
                count -= 1;
                if count == 0 {
                    end = Some(i + 1);
                    break;
                }
            }
        }
    }
    end.map(|e| (text[start..start + e].to_string(), start + e))
}
