use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::time::Instant;

use super::auth;
use super::message;
use super::types::MarshalBuffer;

/// Default deadline for a method call's reply. GNOME Shell and the portal
/// answer in milliseconds; anything past this means the peer is wedged, and
/// blocking forever would wedge the calling agent with us.
const METHOD_CALL_TIMEOUT_MS: u64 = 10_000;

/// D-Bus specifies 128 MiB as the maximum message length. A rogue or
/// corrupted peer that claims a multi-gigabyte fields/body length must
/// produce a JSON error, not an OOM-abort from a giant `vec![0u8; n]`.
const MAX_MESSAGE_SIZE: usize = 128 * 1024 * 1024;

#[allow(dead_code)]
pub struct DbusConnection {
    stream: UnixStream,
    serial: u32,
    unique_name: String,
}

#[allow(dead_code)]
impl DbusConnection {
    pub fn connect() -> Result<Self, String> {
        let mut stream = connect_session_bus()?;

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
    /// once the deadline passes. Delegates the per-read timeout arming to
    /// `read_exact_deadline` for each of the message's four reads, so a
    /// peer that drips bytes slowly cannot stretch a single message past
    /// the deadline (see that function's doc comment).
    fn read_next_message_before(&mut self, deadline: std::time::Instant) -> Result<Reply, String> {
        if std::time::Instant::now() >= deadline {
            return Err("D-Bus read timed out".to_string());
        }
        self.read_next_message(deadline)
    }

    fn read_next_message(&mut self, deadline: std::time::Instant) -> Result<Reply, String> {
        let mut header_buf = [0u8; 16];
        read_exact_deadline(&mut self.stream, &mut header_buf, deadline)?;

        let fields_len = u32::from_le_bytes([
            header_buf[12], header_buf[13], header_buf[14], header_buf[15]
        ]) as usize;
        if fields_len > MAX_MESSAGE_SIZE {
            return Err(format!("D-Bus message too large: {} bytes", fields_len));
        }

        let mut fields_buf = vec![0u8; fields_len];
        if fields_len > 0 {
            read_exact_deadline(&mut self.stream, &mut fields_buf, deadline)?;
        }

        let total_header = 16 + fields_len;
        let padded_header = (total_header + 7) & !7;
        let padding = padded_header - total_header;
        if padding > 0 {
            let mut pad = vec![0u8; padding];
            read_exact_deadline(&mut self.stream, &mut pad, deadline)?;
        }

        let mut full_header = Vec::with_capacity(16 + fields_len);
        full_header.extend_from_slice(&header_buf);
        full_header.extend_from_slice(&fields_buf);

        let (header, _) = message::parse_header(&full_header)?;

        let body_len = header.body_len as usize;
        if body_len > MAX_MESSAGE_SIZE {
            return Err(format!("D-Bus message too large: {} bytes", body_len));
        }
        let mut body = vec![0u8; body_len];
        if body_len > 0 {
            read_exact_deadline(&mut self.stream, &mut body, deadline)?;
        }

        Ok(Reply { header, body })
    }
}

/// read_exact with a hard deadline: re-arms the socket timeout from the
/// remaining budget before every read() and checks the clock between
/// reads, so a drip-feeding peer cannot stretch one message past the
/// deadline (read_exact alone resets the timeout with every byte).
fn read_exact_deadline(stream: &mut UnixStream, buf: &mut [u8], deadline: Instant) -> Result<(), String> {
    let mut filled = 0;
    while filled < buf.len() {
        let remaining = deadline.checked_duration_since(Instant::now())
            .ok_or("D-Bus read timed out")?;
        stream.set_read_timeout(Some(remaining)).map_err(|e| e.to_string())?;
        match stream.read(&mut buf[filled..]) {
            Ok(0) => return Err("D-Bus connection closed mid-message".to_string()),
            Ok(n) => filled += n,
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock
                   || e.kind() == std::io::ErrorKind::TimedOut =>
                return Err("D-Bus read timed out".to_string()),
            Err(e) => return Err(format!("D-Bus read error: {}", e)),
        }
    }
    Ok(())
}

#[allow(dead_code)]
pub struct Reply {
    pub header: message::MessageHeader,
    pub body: Vec<u8>,
}

/// Connect to the session bus. DBUS_SESSION_BUS_ADDRESS is a ';'-separated
/// list of addresses, each "transport:key=val,key=val" — try each in order.
/// Abstract sockets need SocketAddrExt (an interior-NUL path is rejected by
/// UnixStream::connect, so the old "\0name" trick can never work).
fn connect_session_bus() -> Result<UnixStream, String> {
    let Ok(addr) = std::env::var("DBUS_SESSION_BUS_ADDRESS") else {
        let path = format!("/run/user/{}/bus", auth::get_uid());
        return UnixStream::connect(&path)
            .map_err(|e| format!("Failed to connect to D-Bus at {}: {}", path, e));
    };

    let mut last_err = format!("no usable transport in '{}'", addr);
    for entry in addr.split(';') {
        for part in entry.split(',') {
            if let Some(path) = part.strip_prefix("unix:path=") {
                match UnixStream::connect(path) {
                    Ok(s) => return Ok(s),
                    Err(e) => last_err = format!("{}: {}", path, e),
                }
            } else if let Some(name) = part.strip_prefix("unix:abstract=") {
                use std::os::linux::net::SocketAddrExt;
                match std::os::unix::net::SocketAddr::from_abstract_name(name.as_bytes()) {
                    Ok(sa) => match UnixStream::connect_addr(&sa) {
                        Ok(s) => return Ok(s),
                        Err(e) => last_err = format!("abstract:{}: {}", name, e),
                    },
                    Err(e) => last_err = format!("abstract:{}: {}", name, e),
                }
            }
        }
    }
    Err(format!("Failed to connect to D-Bus session bus ({})", last_err))
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

    #[test]
    fn test_read_exact_deadline_trips_on_drip_feeding_peer() {
        // read_exact alone resets its OS-level read timeout on every byte
        // received, so a peer sending 1 byte per 200ms against a 500ms
        // per-read timeout would never itself time out even though the
        // *message* deadline has long passed. read_exact_deadline must
        // check the wall clock against the deadline on every iteration,
        // not just once per call, so the drip cannot outlast the deadline.
        let (mut local, mut remote) = UnixStream::pair().unwrap();
        let writer = std::thread::spawn(move || {
            for _ in 0..10 {
                std::thread::sleep(std::time::Duration::from_millis(200));
                if remote.write_all(&[0u8]).is_err() {
                    return;
                }
            }
        });

        let deadline = Instant::now() + std::time::Duration::from_millis(500);
        let mut buf = [0u8; 10];
        let start = Instant::now();
        let result = read_exact_deadline(&mut local, &mut buf, deadline);
        assert!(result.is_err(), "drip-fed read must time out, not eventually succeed");
        let err = result.err().unwrap();
        assert!(err.contains("timed out"), "error must mention timeout, got: {}", err);
        assert!(start.elapsed().as_millis() < 2_000, "must not wait past ~2s in this test");

        drop(local);
        let _ = writer.join();
    }
}
