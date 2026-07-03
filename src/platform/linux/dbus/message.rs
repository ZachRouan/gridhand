use super::types::{complete_type_len, type_alignment, MarshalBuffer};

const PROTOCOL_VERSION: u8 = 1;

#[allow(dead_code)]
pub const METHOD_CALL: u8 = 1;
#[allow(dead_code)]
pub const METHOD_RETURN: u8 = 2;
#[allow(dead_code)]
pub const ERROR: u8 = 3;
pub const SIGNAL: u8 = 4;

const FIELD_PATH: u8 = 1;
const FIELD_INTERFACE: u8 = 2;
const FIELD_MEMBER: u8 = 3;
const FIELD_ERROR_NAME: u8 = 4;
const FIELD_REPLY_SERIAL: u8 = 5;
const FIELD_DESTINATION: u8 = 6;
#[allow(dead_code)]
const FIELD_SENDER: u8 = 7;
const FIELD_SIGNATURE: u8 = 8;

const NO_REPLY_EXPECTED: u8 = 0x01;

#[allow(clippy::too_many_arguments)]
pub fn build_method_call(
    serial: u32,
    destination: &str,
    path: &str,
    interface: &str,
    member: &str,
    signature: Option<&str>,
    body: &[u8],
    flags: u8,
) -> Vec<u8> {
    let mut fields = MarshalBuffer::new();

    write_header_field(&mut fields, FIELD_PATH, "o", |buf| buf.write_object_path(path));
    write_header_field(&mut fields, FIELD_INTERFACE, "s", |buf| buf.write_string(interface));
    write_header_field(&mut fields, FIELD_MEMBER, "s", |buf| buf.write_string(member));
    write_header_field(&mut fields, FIELD_DESTINATION, "s", |buf| buf.write_string(destination));

    if let Some(sig) = signature {
        write_header_field(&mut fields, FIELD_SIGNATURE, "g", |buf| buf.write_signature(sig));
    }

    let fields_bytes = fields.into_bytes();

    let mut msg = vec![b'l', METHOD_CALL, flags, PROTOCOL_VERSION];
    msg.extend_from_slice(&(body.len() as u32).to_le_bytes());
    msg.extend_from_slice(&serial.to_le_bytes());
    msg.extend_from_slice(&(fields_bytes.len() as u32).to_le_bytes());
    msg.extend_from_slice(&fields_bytes);
    while !msg.len().is_multiple_of(8) {
        msg.push(0);
    }
    msg.extend_from_slice(body);

    msg
}

#[allow(dead_code)]
pub fn build_method_call_no_reply(
    serial: u32,
    destination: &str,
    path: &str,
    interface: &str,
    member: &str,
    signature: Option<&str>,
    body: &[u8],
) -> Vec<u8> {
    build_method_call(serial, destination, path, interface, member, signature, body, NO_REPLY_EXPECTED)
}

fn write_header_field(buf: &mut MarshalBuffer, code: u8, sig: &str, write_val: impl FnOnce(&mut MarshalBuffer)) {
    buf.align_struct();
    buf.write_byte(code);
    buf.write_signature(sig);
    write_val(buf);
}

#[allow(dead_code)]
pub struct MessageHeader {
    pub msg_type: u8,
    pub flags: u8,
    pub body_len: u32,
    pub serial: u32,
    pub reply_serial: Option<u32>,
    pub sender: Option<String>,
    pub path: Option<String>,
    pub interface: Option<String>,
    pub member: Option<String>,
    pub error_name: Option<String>,
    pub signature: Option<String>,
}

pub fn parse_header(data: &[u8]) -> Result<(MessageHeader, usize), String> {
    if data.len() < 16 {
        return Err("Message too short for header".to_string());
    }

    let endian = data[0];
    if endian != b'l' {
        return Err(format!("Unsupported endianness: {:02x} (only little-endian supported)", endian));
    }

    let msg_type = data[1];
    let flags = data[2];
    let _version = data[3];
    let body_len = u32::from_le_bytes([data[4], data[5], data[6], data[7]]);
    let serial = u32::from_le_bytes([data[8], data[9], data[10], data[11]]);
    let fields_len = u32::from_le_bytes([data[12], data[13], data[14], data[15]]) as usize;

    let fields_start = 16;
    let fields_end = fields_start + fields_len;
    if data.len() < fields_end {
        return Err("Message too short for header fields".to_string());
    }

    let mut header = MessageHeader {
        msg_type,
        flags,
        body_len,
        serial,
        reply_serial: None,
        sender: None,
        path: None,
        interface: None,
        member: None,
        error_name: None,
        signature: None,
    };

    let mut pos = fields_start;
    while pos < fields_end {
        while pos % 8 != 0 && pos < fields_end {
            pos += 1;
        }
        if pos >= fields_end {
            break;
        }

        let field_code = data[pos];
        pos += 1;

        if pos >= fields_end { break; }
        let sig_len = data[pos] as usize;
        pos += 1;
        if pos + sig_len + 1 > fields_end {
            return Err("Truncated header field signature".to_string());
        }
        let sig = std::str::from_utf8(&data[pos..pos + sig_len]).unwrap_or("");
        pos += sig_len + 1;

        match (field_code, sig) {
            (FIELD_PATH, "o") | (FIELD_INTERFACE, "s") | (FIELD_MEMBER, "s") |
            (FIELD_ERROR_NAME, "s") | (FIELD_DESTINATION, "s") | (FIELD_SENDER, "s") => {
                while pos % 4 != 0 { pos += 1; }
                if pos + 4 > data.len() {
                    return Err("Truncated header field value".to_string());
                }
                let str_len = u32::from_le_bytes([data[pos], data[pos+1], data[pos+2], data[pos+3]]) as usize;
                pos += 4;
                if pos + str_len + 1 > data.len() {
                    return Err("Truncated header field value".to_string());
                }
                let val = String::from_utf8_lossy(&data[pos..pos + str_len]).to_string();
                pos += str_len + 1;

                match field_code {
                    FIELD_PATH => header.path = Some(val),
                    FIELD_INTERFACE => header.interface = Some(val),
                    FIELD_MEMBER => header.member = Some(val),
                    FIELD_ERROR_NAME => header.error_name = Some(val),
                    FIELD_SENDER => header.sender = Some(val),
                    FIELD_DESTINATION => {}
                    _ => {}
                }
            }
            (FIELD_REPLY_SERIAL, "u") => {
                while pos % 4 != 0 { pos += 1; }
                if pos + 4 > data.len() {
                    return Err("Truncated header field value".to_string());
                }
                let v = u32::from_le_bytes([data[pos], data[pos+1], data[pos+2], data[pos+3]]);
                pos += 4;
                header.reply_serial = Some(v);
            }
            (FIELD_SIGNATURE, "g") => {
                if pos >= data.len() {
                    return Err("Truncated header field value".to_string());
                }
                let slen = data[pos] as usize;
                pos += 1;
                if pos + slen + 1 > data.len() {
                    return Err("Truncated header field value".to_string());
                }
                header.signature = Some(String::from_utf8_lossy(&data[pos..pos + slen]).to_string());
                pos += slen + 1;
            }
            _ => {
                // Unknown field code (or unexpected signature): skip its value
                // by signature. Field order is not guaranteed by the spec, so
                // aborting here could drop REPLY_SERIAL and orphan the reply.
                pos = skip_value(data, pos, sig, 0)?;
            }
        }
    }

    let mut total = fields_end;
    while !total.is_multiple_of(8) {
        total += 1;
    }

    Ok((header, total))
}

/// Skip one complete value of the given signature starting at `pos`,
/// returning the position just past it. Used for header fields this client
/// doesn't understand; every access is bounds-checked so malformed data
/// errors instead of panicking.
fn skip_value(data: &[u8], mut pos: usize, sig: &str, depth: u8) -> Result<usize, String> {
    if depth > 32 {
        return Err("D-Bus signature nesting too deep".to_string());
    }
    let err = || "Truncated header field value".to_string();
    let first = *sig.as_bytes().first().ok_or_else(err)?;

    let align = |pos: usize, n: usize| (pos + n - 1) & !(n - 1);

    match first {
        b'y' => pos += 1,
        b'n' | b'q' => pos = align(pos, 2) + 2,
        b'b' | b'i' | b'u' | b'h' => pos = align(pos, 4) + 4,
        b'x' | b't' | b'd' => pos = align(pos, 8) + 8,
        b's' | b'o' => {
            pos = align(pos, 4);
            if pos + 4 > data.len() { return Err(err()); }
            let len = u32::from_le_bytes([data[pos], data[pos+1], data[pos+2], data[pos+3]]) as usize;
            pos += 4 + len + 1;
        }
        b'g' => {
            if pos >= data.len() { return Err(err()); }
            let len = data[pos] as usize;
            pos += 1 + len + 1;
        }
        b'a' => {
            pos = align(pos, 4);
            if pos + 4 > data.len() { return Err(err()); }
            let len = u32::from_le_bytes([data[pos], data[pos+1], data[pos+2], data[pos+3]]) as usize;
            pos += 4;
            pos = align(pos, type_alignment(&sig[1..]));
            pos += len;
        }
        b'v' => {
            if pos >= data.len() { return Err(err()); }
            let slen = data[pos] as usize;
            pos += 1;
            if pos + slen + 1 > data.len() { return Err(err()); }
            let inner = std::str::from_utf8(&data[pos..pos + slen])
                .map_err(|_| "Invalid signature in variant".to_string())?
                .to_string();
            pos += slen + 1;
            pos = skip_value(data, pos, &inner, depth + 1)?;
        }
        b'(' | b'{' => {
            pos = align(pos, 8);
            let inner = &sig[1..sig.len() - 1];
            let mut rest = inner;
            while !rest.is_empty() {
                let tlen = complete_type_len(rest)?;
                pos = skip_value(data, pos, &rest[..tlen], depth + 1)?;
                rest = &rest[tlen..];
            }
        }
        other => return Err(format!("Unsupported type in signature: {}", other as char)),
    }

    if pos > data.len() {
        return Err(err());
    }
    Ok(pos)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a 16-byte fixed header followed by the given field bytes.
    fn make_header(msg_type: u8, fields: &[u8]) -> Vec<u8> {
        let mut data = vec![b'l', msg_type, 0, PROTOCOL_VERSION];
        data.extend_from_slice(&0u32.to_le_bytes()); // body length
        data.extend_from_slice(&1u32.to_le_bytes()); // serial
        data.extend_from_slice(&(fields.len() as u32).to_le_bytes());
        data.extend_from_slice(fields);
        data
    }

    #[test]
    fn test_parse_header_skips_unknown_field() {
        // Field 1: code 9 (UNIX_FDS, unknown to us), sig "u", value 1.
        // Field 2: REPLY_SERIAL = 42. The spec does not guarantee field order,
        // so an unknown field must be skipped, not abort the loop — otherwise
        // the reply never matches and the caller waits forever.
        let mut fields = vec![9u8, 1, b'u', 0]; // code, siglen, 'u', NUL (pos now 4-aligned)
        fields.extend_from_slice(&1u32.to_le_bytes());
        fields.extend_from_slice(&[FIELD_REPLY_SERIAL, 1, b'u', 0]);
        fields.extend_from_slice(&42u32.to_le_bytes());

        let data = make_header(METHOD_RETURN, &fields);
        let (header, _) = parse_header(&data).unwrap();
        assert_eq!(header.reply_serial, Some(42));
    }

    #[test]
    fn test_parse_header_skips_unknown_string_field() {
        // Unknown field with a string-typed variant before REPLY_SERIAL:
        // the skipper must consume the value by its signature, not guess.
        let mut fields = vec![200u8, 1, b's', 0]; // fictional code, sig "s"
        fields.extend_from_slice(&5u32.to_le_bytes());
        fields.extend_from_slice(b"hello\0");
        // pad to 8 for next struct entry (currently at 4+4+6=14 -> 16)
        fields.extend_from_slice(&[0, 0]);
        fields.extend_from_slice(&[FIELD_REPLY_SERIAL, 1, b'u', 0]);
        fields.extend_from_slice(&7u32.to_le_bytes());

        let data = make_header(METHOD_RETURN, &fields);
        let (header, _) = parse_header(&data).unwrap();
        assert_eq!(header.reply_serial, Some(7));
    }

    #[test]
    fn test_parse_header_normal_fields_still_work() {
        // REPLY_SERIAL followed by SENDER, the common reply shape.
        let mut fields = vec![FIELD_REPLY_SERIAL, 1, b'u', 0];
        fields.extend_from_slice(&3u32.to_le_bytes());
        fields.extend_from_slice(&[FIELD_SENDER, 1, b's', 0]);
        fields.extend_from_slice(&4u32.to_le_bytes());
        fields.extend_from_slice(b":1.5\0");

        let data = make_header(METHOD_RETURN, &fields);
        let (header, _) = parse_header(&data).unwrap();
        assert_eq!(header.reply_serial, Some(3));
        assert_eq!(header.sender.as_deref(), Some(":1.5"));
    }
}
