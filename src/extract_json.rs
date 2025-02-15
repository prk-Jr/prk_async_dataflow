
pub fn extract_json(bytes: &[u8]) -> Option<(&[u8], usize)> {
    let bytes = skip_whitespace(bytes);
    if bytes.is_empty() {
        return None;
    }

    // Find the start of the JSON object or array
    let start = bytes.iter().position(|&c| c == b'{' || c == b'[')?;
    let bytes_slice = &bytes[start..];

    // Extract the JSON object or array
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
    let mut end = None;

    for (i, &c) in bytes_slice.iter().enumerate() {
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

// New function to handle both single Post and Vec<Post>
use simd_json::OwnedValue;

pub fn extract_json_value(bytes: &[u8]) -> Option<OwnedValue> {
    if let Some((json, _)) = extract_json(bytes) {
        // Convert the slice to a mutable Vec<u8> (required by simd_json)
        let mut json_vec = json.to_vec();
        // Parse the JSON using simd_json
        simd_json::to_owned_value(&mut json_vec).ok()
    } else {
        None
    }
}