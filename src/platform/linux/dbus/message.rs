use super::types::MarshalBuffer;

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

    let mut msg = Vec::new();
    msg.push(b'l');
    msg.push(METHOD_CALL);
    msg.push(flags);
    msg.push(PROTOCOL_VERSION);
    msg.extend_from_slice(&(body.len() as u32).to_le_bytes());
    msg.extend_from_slice(&serial.to_le_bytes());
    msg.extend_from_slice(&(fields_bytes.len() as u32).to_le_bytes());
    msg.extend_from_slice(&fields_bytes);
    while msg.len() % 8 != 0 {
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
        if pos + sig_len + 1 > fields_end { break; }
        let sig = std::str::from_utf8(&data[pos..pos + sig_len]).unwrap_or("");
        pos += sig_len + 1;

        match (field_code, sig) {
            (FIELD_PATH, "o") | (FIELD_INTERFACE, "s") | (FIELD_MEMBER, "s") |
            (FIELD_ERROR_NAME, "s") | (FIELD_DESTINATION, "s") | (FIELD_SENDER, "s") => {
                while pos % 4 != 0 { pos += 1; }
                if pos + 4 > data.len() { break; }
                let str_len = u32::from_le_bytes([data[pos], data[pos+1], data[pos+2], data[pos+3]]) as usize;
                pos += 4;
                if pos + str_len + 1 > data.len() { break; }
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
                if pos + 4 > data.len() { break; }
                let v = u32::from_le_bytes([data[pos], data[pos+1], data[pos+2], data[pos+3]]);
                pos += 4;
                header.reply_serial = Some(v);
            }
            (FIELD_SIGNATURE, "g") => {
                if pos >= data.len() { break; }
                let slen = data[pos] as usize;
                pos += 1;
                if pos + slen + 1 > data.len() { break; }
                header.signature = Some(String::from_utf8_lossy(&data[pos..pos + slen]).to_string());
                pos += slen + 1;
            }
            _ => {
                break;
            }
        }
    }

    let mut total = fields_end;
    while total % 8 != 0 {
        total += 1;
    }

    Ok((header, total))
}
