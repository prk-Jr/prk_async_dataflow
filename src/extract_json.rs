/// Enhanced version with better error reporting
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

    let mut count = 0i32;
    let mut in_string = false;
    let mut escape = false;
    let mut end = None;

    for (i, c) in text_slice.char_indices() {
        match (in_string, escape, c) {
            (true, false, '\\') => escape = true,
            (true, false, '"') => in_string = false,
            (false, _, '"') => in_string = true,
            (false, _, c) if c == opening => count += 1,
            (false, _, c) if c == closing => {
                count -= 1;
                if count == 0 {
                    end = Some(i + 1);
                    break;
                }
            }
            _ => {}
        }
        
        if escape && c != '\\' {
            escape = false;
        }
    }

    end.map(|e| {
        let json_str = text[start..start + e].to_string();
        (json_str, start + e)
    })
}