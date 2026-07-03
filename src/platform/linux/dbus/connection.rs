use std::io::{Read, Write};
use std::os::unix::net::UnixStream;

use super::auth;
use super::message;
use super::types::MarshalBuffer;

/// Default deadline for a method call's reply. GNOME Shell and the portal
/// answer in milliseconds; anything past this means the peer is wedged, and
/// blocking forever would wedge the calling agent with us.
const METHOD_CALL_TIMEOUT_MS: u64 = 10_000;

#[allow(dead_code)]
pub struct DbusConnection {
    stream: UnixStream,
    serial: u32,
    unique_name: String,
}

#[allow(dead_code)]
impl DbusConnection {
    pub fn connect() -> Result<Self, String> {
        let path = get_session_bus_path()?;
        let mut stream = UnixStream::connect(&path)
            .map_err(|e| format!("Failed to connect to D-Bus at {}: {}", path, e))?;

        auth::authenticate(&mut stream)?;

        let mut conn = DbusConnection {
            stream,
            serial: 0,
            unique_name: String::new(),
        };

        let reply = conn.call_method(
            "org.freedesktop.DBus",
            "/org/freedesktop/DBus",
            "org.freedesktop.DBus",
            "Hello",
            None,
            &[],
        )?;

        let mut ubuf = super::types::UnmarshalBuffer::new(&reply.body);
        conn.unique_name = ubuf.read_string()?;

        Ok(conn)
    }

    pub fn unique_name(&self) -> &str {
        &self.unique_name
    }

    pub fn call_method(
        &mut self,
        destination: &str,
        path: &str,
        interface: &str,
        member: &str,
        signature: Option<&str>,
        body: &[u8],
    ) -> Result<Reply, String> {
        self.call_method_with_timeout(destination, path, interface, member, signature, body, METHOD_CALL_TIMEOUT_MS)
    }

    #[allow(clippy::too_many_arguments)]
    fn call_method_with_timeout(
        &mut self,
        destination: &str,
        path: &str,
        interface: &str,
        member: &str,
        signature: Option<&str>,
        body: &[u8],
        timeout_ms: u64,
    ) -> Result<Reply, String> {
        self.serial += 1;
        let msg = message::build_method_call(
            self.serial,
            destination,
            path,
            interface,
            member,
            signature,
            body,
            0,
        );

        self.stream.write_all(&msg)
            .map_err(|e| format!("Failed to send D-Bus message: {}", e))?;

        let deadline = std::time::Instant::now() + std::time::Duration::from_millis(timeout_ms);
        self.read_reply(self.serial, deadline, timeout_ms)
    }

    pub fn call_method_no_reply(
        &mut self,
        destination: &str,
        path: &str,
        interface: &str,
        member: &str,
        signature: Option<&str>,
        body: &[u8],
    ) -> Result<(), String> {
        self.serial += 1;
        let msg = message::build_method_call_no_reply(
            self.serial,
            destination,
            path,
            interface,
            member,
            signature,
            body,
        );

        self.stream.write_all(&msg)
            .map_err(|e| format!("Failed to send D-Bus message: {}", e))
    }

    pub fn add_match(&mut self, rule: &str) -> Result<(), String> {
        let mut body = MarshalBuffer::new();
        body.write_string(rule);

        self.call_method(
            "org.freedesktop.DBus",
            "/org/freedesktop/DBus",
            "org.freedesktop.DBus",
            "AddMatch",
            Some("s"),
            &body.into_bytes(),
        )?;

        Ok(())
    }

    pub fn wait_for_signal(
        &mut self,
        expected_path: &str,
        expected_interface: &str,
        expected_member: &str,
        timeout_ms: u64,
    ) -> Result<Reply, String> {
        let deadline = std::time::Instant::now() + std::time::Duration::from_millis(timeout_ms);
        loop {
            let reply = match self.read_next_message_before(deadline) {
                Ok(r) => r,
                Err(e) if e.contains("timed out") => {
                    return Err("Timeout waiting for signal".to_string());
                }
                Err(e) => return Err(e),
            };

            if reply.header.msg_type == message::SIGNAL {
                let path_match = reply.header.path.as_deref() == Some(expected_path);
                let iface_match = reply.header.interface.as_deref() == Some(expected_interface);
                let member_match = reply.header.member.as_deref() == Some(expected_member);
                if path_match && iface_match && member_match {
                    return Ok(reply);
                }
            }
        }
    }

    fn read_reply(&mut self, expected_serial: u32, deadline: std::time::Instant, timeout_ms: u64) -> Result<Reply, String> {
        loop {
            let reply = match self.read_next_message_before(deadline) {
                Ok(r) => r,
                Err(e) if e.contains("timed out") => {
                    return Err(format!("D-Bus method call timed out after {}ms", timeout_ms));
                }
                Err(e) => return Err(e),
            };

            if let Some(rs) = reply.header.reply_serial
                && rs == expected_serial {
                    if reply.header.msg_type == message::ERROR {
                        let error_name = reply.header.error_name.clone().unwrap_or_default();
                        let mut msg = error_name.clone();
                        if !reply.body.is_empty()
                            && let Ok(s) = super::types::UnmarshalBuffer::new(&reply.body).read_string() {
                                msg = format!("{}: {}", error_name, s);
                            }
                        return Err(msg);
                    }
                    return Ok(reply);
                }
        }
    }

    /// Read one message, giving up (with an error containing "timed out")
    /// once the deadline passes. The read timeout is re-armed with the
    /// remaining time before each read so the total wait honors the deadline.
    fn read_next_message_before(&mut self, deadline: std::time::Instant) -> Result<Reply, String> {
        let now = std::time::Instant::now();
        if now >= deadline {
            return Err("D-Bus read timed out".to_string());
        }
        self.stream.set_read_timeout(Some(deadline - now))
            .map_err(|e| format!("Failed to set read timeout: {}", e))?;
        self.read_next_message()
    }

    fn read_exact_or_timeout(&mut self, buf: &mut [u8], what: &str) -> Result<(), String> {
        self.stream.read_exact(buf).map_err(|e| match e.kind() {
            std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut => {
                "D-Bus read timed out".to_string()
            }
            _ => format!("Failed to read {}: {}", what, e),
        })
    }

    fn read_next_message(&mut self) -> Result<Reply, String> {
        let mut header_buf = [0u8; 16];
        self.read_exact_or_timeout(&mut header_buf, "D-Bus message header")?;

        let fields_len = u32::from_le_bytes([
            header_buf[12], header_buf[13], header_buf[14], header_buf[15]
        ]) as usize;

        let mut fields_buf = vec![0u8; fields_len];
        if fields_len > 0 {
            self.read_exact_or_timeout(&mut fields_buf, "header fields")?;
        }

        let total_header = 16 + fields_len;
        let padded_header = (total_header + 7) & !7;
        let padding = padded_header - total_header;
        if padding > 0 {
            let mut pad = vec![0u8; padding];
            self.read_exact_or_timeout(&mut pad, "header padding")?;
        }

        let mut full_header = Vec::with_capacity(16 + fields_len);
        full_header.extend_from_slice(&header_buf);
        full_header.extend_from_slice(&fields_buf);

        let (header, _) = message::parse_header(&full_header)?;

        let body_len = header.body_len as usize;
        let mut body = vec![0u8; body_len];
        if body_len > 0 {
            self.read_exact_or_timeout(&mut body, "message body")?;
        }

        Ok(Reply { header, body })
    }
}

#[allow(dead_code)]
pub struct Reply {
    pub header: message::MessageHeader,
    pub body: Vec<u8>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_call_method_times_out_on_silent_peer() {
        // A peer that never replies must produce a timeout error, not block
        // this process (and the agent driving it) forever.
        let (local, _remote) = UnixStream::pair().unwrap();
        let mut conn = DbusConnection {
            stream: local,
            serial: 0,
            unique_name: String::new(),
        };

        let start = std::time::Instant::now();
        let result = conn.call_method_with_timeout(
            "org.example", "/", "org.example", "Ping", None, &[], 200,
        );
        assert!(result.is_err(), "silent peer must yield an error");
        let err = result.err().unwrap();
        assert!(err.contains("timed out"), "error must mention timeout, got: {}", err);
        assert!(start.elapsed().as_millis() < 5_000, "must return promptly");
    }
}

fn get_session_bus_path() -> Result<String, String> {
    if let Ok(addr) = std::env::var("DBUS_SESSION_BUS_ADDRESS") {
        for part in addr.split(',') {
            if let Some(path) = part.strip_prefix("unix:path=") {
                return Ok(path.to_string());
            }
            if let Some(rest) = part.strip_prefix("unix:abstract=") {
                return Ok(format!("\0{}", rest));
            }
        }
        Err(format!("Cannot parse DBUS_SESSION_BUS_ADDRESS: {}", addr))
    } else {
        let uid = auth::get_uid();
        Ok(format!("/run/user/{}/bus", uid))
    }
}
