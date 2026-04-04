use std::io::{Read, Write};
use std::os::unix::net::UnixStream;

/// Perform SASL EXTERNAL authentication on the D-Bus socket.
pub fn authenticate(stream: &mut UnixStream) -> Result<(), String> {
    stream.write_all(&[0u8])
        .map_err(|e| format!("Failed to send NUL byte: {}", e))?;

    let uid = get_uid();
    let uid_str = uid.to_string();
    let uid_hex: String = uid_str.bytes().map(|b| format!("{:02x}", b)).collect();

    let auth_cmd = format!("AUTH EXTERNAL {}\r\n", uid_hex);
    stream.write_all(auth_cmd.as_bytes())
        .map_err(|e| format!("Failed to send AUTH: {}", e))?;

    let reply = read_line(stream)?;
    if !reply.starts_with("OK ") {
        return Err(format!("Authentication failed: {}", reply));
    }

    stream.write_all(b"BEGIN\r\n")
        .map_err(|e| format!("Failed to send BEGIN: {}", e))?;

    Ok(())
}

fn read_line(stream: &mut UnixStream) -> Result<String, String> {
    let mut buf = Vec::new();
    let mut byte = [0u8; 1];
    loop {
        stream.read_exact(&mut byte)
            .map_err(|e| format!("Failed to read auth reply: {}", e))?;
        if byte[0] == b'\n' {
            break;
        }
        buf.push(byte[0]);
    }
    if buf.last() == Some(&b'\r') {
        buf.pop();
    }
    String::from_utf8(buf).map_err(|e| format!("Invalid UTF-8 in auth reply: {}", e))
}

pub fn get_uid() -> u32 {
    let ret: u64;
    unsafe {
        std::arch::asm!(
            "syscall",
            in("rax") 102u64,
            lateout("rax") ret,
            out("rcx") _,
            out("r11") _,
            options(nostack, nomem),
        );
    }
    ret as u32
}
