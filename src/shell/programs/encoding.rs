//! Encoding utility programs

use super::{args_to_strs, check_help, read_file_content};

/// Base64 encode or decode
pub fn prog_base64(args: &[String], stdin: &str, stdout: &mut String, stderr: &mut String) -> i32 {
    let args = args_to_strs(args);

    if let Some(help) = check_help(
        &args,
        "Usage: base64 [-d] [FILE]\nBase64 encode or decode.\n  -d  Decode",
    ) {
        stdout.push_str(&help);
        return 0;
    }

    let decode = args.iter().any(|a| *a == "-d" || *a == "--decode");
    let file_args: Vec<&str> = args
        .iter()
        .filter(|a| !a.starts_with('-'))
        .map(|s| s.as_ref())
        .collect();

    let input = if let Some(file) = file_args.first() {
        match read_file_content(file) {
            Ok(c) => c,
            Err(e) => {
                stderr.push_str(&format!("base64: {}: {}\n", file, e));
                return 1;
            }
        }
    } else {
        stdin.to_string()
    };

    if decode {
        // Simple base64 decode
        let chars: Vec<char> = input.chars().filter(|c| !c.is_whitespace()).collect();
        let mut result = Vec::new();
        let mut i = 0;

        while i < chars.len() {
            let chunk: Vec<u8> = chars[i..]
                .iter()
                .take(4)
                .map(|c| base64_decode_char(*c))
                .collect();
            if chunk.len() < 4 {
                break;
            }

            let val = ((chunk[0] as u32) << 18)
                | ((chunk[1] as u32) << 12)
                | ((chunk[2] as u32) << 6)
                | (chunk[3] as u32);
            result.push((val >> 16) as u8);
            if chunk[2] < 64 {
                result.push((val >> 8) as u8);
            }
            if chunk[3] < 64 {
                result.push(val as u8);
            }
            i += 4;
        }

        if let Ok(s) = String::from_utf8(result) {
            stdout.push_str(&s);
        } else {
            stderr.push_str("base64: invalid encoding\n");
            return 1;
        }
    } else {
        // Base64 encode
        let bytes = input.as_bytes();
        let mut result = String::new();

        for chunk in bytes.chunks(3) {
            let val = match chunk.len() {
                3 => ((chunk[0] as u32) << 16) | ((chunk[1] as u32) << 8) | (chunk[2] as u32),
                2 => ((chunk[0] as u32) << 16) | ((chunk[1] as u32) << 8),
                1 => (chunk[0] as u32) << 16,
                _ => break,
            };

            result.push(base64_encode_val((val >> 18) & 0x3F));
            result.push(base64_encode_val((val >> 12) & 0x3F));
            result.push(if chunk.len() > 1 {
                base64_encode_val((val >> 6) & 0x3F)
            } else {
                '='
            });
            result.push(if chunk.len() > 2 {
                base64_encode_val(val & 0x3F)
            } else {
                '='
            });
        }

        stdout.push_str(&result);
        stdout.push('\n');
    }

    0
}

fn base64_encode_val(v: u32) -> char {
    const CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    CHARS[v as usize] as char
}

fn base64_decode_char(c: char) -> u8 {
    match c {
        'A'..='Z' => (c as u8) - b'A',
        'a'..='z' => (c as u8) - b'a' + 26,
        '0'..='9' => (c as u8) - b'0' + 52,
        '+' => 62,
        '/' => 63,
        '=' => 64, // padding
        _ => 64,
    }
}

/// xxd - hex dump
pub fn prog_xxd(args: &[String], stdin: &str, stdout: &mut String, stderr: &mut String) -> i32 {
    let args = args_to_strs(args);

    if let Some(help) = check_help(&args, "Usage: xxd [FILE]\nMake a hexdump.") {
        stdout.push_str(&help);
        return 0;
    }

    let input = if let Some(file) = args.first() {
        match read_file_content(file) {
            Ok(c) => c,
            Err(e) => {
                stderr.push_str(&format!("xxd: {}: {}\n", file, e));
                return 1;
            }
        }
    } else {
        stdin.to_string()
    };

    let bytes = input.as_bytes();

    for (offset, chunk) in bytes.chunks(16).enumerate() {
        // Offset
        stdout.push_str(&format!("{:08x}: ", offset * 16));

        // Hex bytes
        for (i, byte) in chunk.iter().enumerate() {
            stdout.push_str(&format!("{:02x}", byte));
            if i % 2 == 1 {
                stdout.push(' ');
            }
        }

        // Padding for incomplete lines
        for i in chunk.len()..16 {
            stdout.push_str("  ");
            if i % 2 == 1 {
                stdout.push(' ');
            }
        }

        // ASCII representation
        stdout.push(' ');
        for byte in chunk {
            if *byte >= 0x20 && *byte < 0x7f {
                stdout.push(*byte as char);
            } else {
                stdout.push('.');
            }
        }
        stdout.push('\n');
    }

    0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_base64_encode() {
        let args = vec![];
        let stdin = "Hello, World!";
        let mut stdout = String::new();
        let mut stderr = String::new();

        let result = prog_base64(&args, stdin, &mut stdout, &mut stderr);

        assert_eq!(result, 0);
        assert_eq!(stdout.trim(), "SGVsbG8sIFdvcmxkIQ==");
        assert_eq!(stderr, "");
    }

    #[test]
    fn test_base64_decode() {
        let args = vec!["-d".to_string()];
        let stdin = "SGVsbG8sIFdvcmxkIQ==";
        let mut stdout = String::new();
        let mut stderr = String::new();

        let result = prog_base64(&args, stdin, &mut stdout, &mut stderr);

        assert_eq!(result, 0);
        assert_eq!(stdout, "Hello, World!");
        assert_eq!(stderr, "");
    }

    #[test]
    fn test_base64_encode_empty() {
        let args = vec![];
        let stdin = "";
        let mut stdout = String::new();
        let mut stderr = String::new();

        let result = prog_base64(&args, stdin, &mut stdout, &mut stderr);

        assert_eq!(result, 0);
        assert_eq!(stdout.trim(), "");
    }

    #[test]
    fn test_base64_encode_val() {
        assert_eq!(base64_encode_val(0), 'A');
        assert_eq!(base64_encode_val(25), 'Z');
        assert_eq!(base64_encode_val(26), 'a');
        assert_eq!(base64_encode_val(51), 'z');
        assert_eq!(base64_encode_val(52), '0');
        assert_eq!(base64_encode_val(61), '9');
        assert_eq!(base64_encode_val(62), '+');
        assert_eq!(base64_encode_val(63), '/');
    }

    #[test]
    fn test_base64_decode_char() {
        assert_eq!(base64_decode_char('A'), 0);
        assert_eq!(base64_decode_char('Z'), 25);
        assert_eq!(base64_decode_char('a'), 26);
        assert_eq!(base64_decode_char('z'), 51);
        assert_eq!(base64_decode_char('0'), 52);
        assert_eq!(base64_decode_char('9'), 61);
        assert_eq!(base64_decode_char('+'), 62);
        assert_eq!(base64_decode_char('/'), 63);
        assert_eq!(base64_decode_char('='), 64);
    }

    #[test]
    fn test_xxd_simple() {
        let args: Vec<String> = vec![];
        let stdin = "Hello";
        let mut stdout = String::new();
        let mut stderr = String::new();

        let result = prog_xxd(&args, stdin, &mut stdout, &mut stderr);

        assert_eq!(result, 0);
        // xxd outputs hex pairs with spaces: "4865 6c6c 6f"
        assert!(stdout.contains("48")); // 'H' = 0x48
        assert!(stdout.contains("65")); // 'e' = 0x65
        assert!(stdout.contains("Hello")); // ASCII representation
        assert_eq!(stderr, "");
    }

    #[test]
    fn test_xxd_empty() {
        let args = vec![];
        let stdin = "";
        let mut stdout = String::new();
        let mut stderr = String::new();

        let result = prog_xxd(&args, stdin, &mut stdout, &mut stderr);

        assert_eq!(result, 0);
        assert_eq!(stdout, "");
    }

    #[test]
    fn test_xxd_multiline() {
        let args = vec![];
        let stdin = "0123456789abcdef0123456789abcdef0";
        let mut stdout = String::new();
        let mut stderr = String::new();

        let result = prog_xxd(&args, stdin, &mut stdout, &mut stderr);

        assert_eq!(result, 0);
        // Should have 2 lines (16 bytes each)
        assert_eq!(stdout.lines().count(), 3); // 16 + 16 + 1 byte
    }
}
