use std::io::{Read, Write};
use std::os::unix::net::UnixStream;

use super::auth;
use super::message;
use super::types::MarshalBuffer;

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

        self.read_reply(self.serial)
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
        self.stream.set_read_timeout(Some(std::time::Duration::from_millis(timeout_ms)))
            .map_err(|e| format!("Failed to set read timeout: {}", e))?;

        let start = std::time::Instant::now();
        loop {
            if start.elapsed().as_millis() as u64 > timeout_ms {
                return Err("Timeout waiting for signal".to_string());
            }

            let reply = self.read_next_message()?;

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

    fn read_reply(&mut self, expected_serial: u32) -> Result<Reply, String> {
        loop {
            let reply = self.read_next_message()?;

            if let Some(rs) = reply.header.reply_serial {
                if rs == expected_serial {
                    if reply.header.msg_type == message::ERROR {
                        let error_name = reply.header.error_name.clone().unwrap_or_default();
                        let mut msg = error_name.clone();
                        if !reply.body.is_empty() {
                            if let Ok(s) = super::types::UnmarshalBuffer::new(&reply.body).read_string() {
                                msg = format!("{}: {}", error_name, s);
                            }
                        }
                        return Err(msg);
                    }
                    return Ok(reply);
                }
            }
        }
    }

    fn read_next_message(&mut self) -> Result<Reply, String> {
        let mut header_buf = [0u8; 16];
        self.stream.read_exact(&mut header_buf)
            .map_err(|e| format!("Failed to read D-Bus message header: {}", e))?;

        let fields_len = u32::from_le_bytes([
            header_buf[12], header_buf[13], header_buf[14], header_buf[15]
        ]) as usize;

        let mut fields_buf = vec![0u8; fields_len];
        if fields_len > 0 {
            self.stream.read_exact(&mut fields_buf)
                .map_err(|e| format!("Failed to read header fields: {}", e))?;
        }

        let total_header = 16 + fields_len;
        let padded_header = (total_header + 7) & !7;
        let padding = padded_header - total_header;
        if padding > 0 {
            let mut pad = vec![0u8; padding];
            self.stream.read_exact(&mut pad)
                .map_err(|e| format!("Failed to read header padding: {}", e))?;
        }

        let mut full_header = Vec::with_capacity(16 + fields_len);
        full_header.extend_from_slice(&header_buf);
        full_header.extend_from_slice(&fields_buf);

        let (header, _) = message::parse_header(&full_header)?;

        let body_len = header.body_len as usize;
        let mut body = vec![0u8; body_len];
        if body_len > 0 {
            self.stream.read_exact(&mut body)
                .map_err(|e| format!("Failed to read message body: {}", e))?;
        }

        Ok(Reply { header, body })
    }
}

#[allow(dead_code)]
pub struct Reply {
    pub header: message::MessageHeader,
    pub body: Vec<u8>,
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
