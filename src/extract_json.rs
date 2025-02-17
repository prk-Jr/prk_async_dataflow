
pub fn extract_json(bytes: &[u8]) -> Option<(&[u8], usize)> {
    let bytes = skip_whitespace(bytes);
    if bytes.is_empty() {
        return None;
    }

    // Find the start of the JSON object or array
    let start = bytes.iter().position(|&c| c == b'{' || c == b'[')?;
    let bytes_slice = &bytes[start..];

    // Enhanced extraction with JSON5 support
    if let Some((json, consumed)) = extract_single_json(bytes_slice) {
        return Some((json, start + consumed));
    }

    None
}

fn skip_whitespace(bytes: &[u8]) -> &[u8] {
    let mut start = 0;
    while start < bytes.len() && bytes[start].is_ascii_whitespace() {
        start += 1;
    }
    &bytes[start..]
}

fn extract_single_json(bytes: &[u8]) -> Option<(&[u8], usize)> {
    let bytes = skip_whitespace(bytes);
    let start = bytes.iter().position(|&c| c == b'{' || c == b'[')?;
    let bytes_slice = &bytes[start..];
    let first_char = bytes_slice[0];

    let (opening, closing) = match first_char {
        b'{' => (b'{', b'}'),
        b'[' => (b'[', b']'),
        _ => return None,
    };

    let mut count = 0i32;
    let mut in_string = false;
    let mut escape = false;
    let mut in_single_line_comment = false;
    let mut in_multi_line_comment = false;
    let mut end = None;

    for (i, &c) in bytes_slice.iter().enumerate() {
        // Handle comments (JSON5 feature)
        if !in_string && !in_multi_line_comment && !in_single_line_comment {
            if c == b'/' && i < bytes_slice.len() - 1 {
                match bytes_slice[i + 1] {
                    b'/' => in_single_line_comment = true,
                    b'*' => in_multi_line_comment = true,
                    _ => {}
                }
            }
        }

        if in_single_line_comment {
            if c == b'\n' {
                in_single_line_comment = false;
            }
            continue;
        }

        if in_multi_line_comment {
            if c == b'*' && i < bytes_slice.len() - 1 && bytes_slice[i + 1] == b'/' {
                in_multi_line_comment = false;
            }
            continue;
        }

        // Main JSON parsing logic
        match (in_string, escape, c) {
            (true, false, b'\\') => escape = true,
            (true, true, _) => escape = false,
            (true, false, b'"') => in_string = false,
            (false, _, b'"') => in_string = true,
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
    }

    end.map(|e| (&bytes_slice[..e], start + e))
}

#[cfg(feature = "relaxed")]
pub fn extract_json5_value(bytes: &[u8]) -> Option<simd_json::OwnedValue> {
    if let Some((json, _)) = extract_json(bytes) {
        let json_str = std::str::from_utf8(json).ok()?;
        json5::from_str(json_str).ok()
    } else {
        None
    }
}