use memchr;

pub fn extract_json(bytes: &[u8]) -> Option<(&[u8], usize)> {
    let bytes = skip_whitespace(bytes);
    if let Some(pos) = memchr::memchr(b'\n', bytes) {
        let line = &bytes[..pos];
        if let Some((json, consumed)) = extract_single_json(line) {
            return Some((json, pos + 1));
        }
    }
    extract_single_json(bytes)
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