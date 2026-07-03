/// A buffer that tracks its write position for D-Bus alignment rules.
#[allow(dead_code)]
pub struct MarshalBuffer {
    pub data: Vec<u8>,
}

#[allow(dead_code)]
impl MarshalBuffer {
    pub fn new() -> Self {
        Self { data: Vec::with_capacity(256) }
    }

    pub fn len(&self) -> usize {
        self.data.len()
    }

    pub fn align(&mut self, alignment: usize) {
        while !self.data.len().is_multiple_of(alignment) {
            self.data.push(0);
        }
    }

    pub fn write_byte(&mut self, b: u8) {
        self.data.push(b);
    }

    pub fn write_u32(&mut self, v: u32) {
        self.align(4);
        self.data.extend_from_slice(&v.to_le_bytes());
    }

    pub fn write_i32(&mut self, v: i32) {
        self.align(4);
        self.data.extend_from_slice(&v.to_le_bytes());
    }

    pub fn write_boolean(&mut self, v: bool) {
        self.write_u32(if v { 1 } else { 0 });
    }

    pub fn write_string(&mut self, s: &str) {
        self.align(4);
        let bytes = s.as_bytes();
        self.data.extend_from_slice(&(bytes.len() as u32).to_le_bytes());
        self.data.extend_from_slice(bytes);
        self.data.push(0);
    }

    pub fn write_object_path(&mut self, s: &str) {
        self.write_string(s);
    }

    pub fn write_signature(&mut self, s: &str) {
        let bytes = s.as_bytes();
        self.data.push(bytes.len() as u8);
        self.data.extend_from_slice(bytes);
        self.data.push(0);
    }

    pub fn write_variant_bool(&mut self, v: bool) {
        self.write_signature("b");
        self.write_boolean(v);
    }

    pub fn write_variant_string(&mut self, v: &str) {
        self.write_signature("s");
        self.write_string(v);
    }

    pub fn write_variant_u32(&mut self, v: u32) {
        self.write_signature("u");
        self.write_u32(v);
    }

    pub fn start_array(&mut self, element_alignment: usize) -> usize {
        self.align(4);
        let len_pos = self.data.len();
        self.data.extend_from_slice(&0u32.to_le_bytes());
        self.align(element_alignment);
        len_pos
    }

    pub fn finish_array(&mut self, len_pos: usize) {
        let len_field_end = len_pos + 4;
        let mut data_start = len_field_end;
        while data_start < self.data.len() && !data_start.is_multiple_of(8) && data_start < len_field_end + 8 {
            data_start += 1;
        }
        if data_start > self.data.len() {
            data_start = len_field_end;
        }
        let array_len = (self.data.len() - data_start) as u32;
        self.data[len_pos..len_pos + 4].copy_from_slice(&array_len.to_le_bytes());
    }

    pub fn align_struct(&mut self) {
        self.align(8);
    }

    pub fn into_bytes(self) -> Vec<u8> {
        self.data
    }
}

/// Read helpers for parsing D-Bus replies.
#[allow(dead_code)]
pub struct UnmarshalBuffer<'a> {
    pub data: &'a [u8],
    pub pos: usize,
}

#[allow(dead_code)]
impl<'a> UnmarshalBuffer<'a> {
    pub fn new(data: &'a [u8]) -> Self {
        Self { data, pos: 0 }
    }

    pub fn remaining(&self) -> usize {
        self.data.len().saturating_sub(self.pos)
    }

    pub fn align(&mut self, alignment: usize) {
        while !self.pos.is_multiple_of(alignment) && self.pos < self.data.len() {
            self.pos += 1;
        }
    }

    pub fn read_byte(&mut self) -> Result<u8, String> {
        if self.pos >= self.data.len() {
            return Err("Unexpected end of data".to_string());
        }
        let b = self.data[self.pos];
        self.pos += 1;
        Ok(b)
    }

    pub fn read_u32(&mut self) -> Result<u32, String> {
        self.align(4);
        if self.pos + 4 > self.data.len() {
            return Err("Unexpected end of data reading u32".to_string());
        }
        let v = u32::from_le_bytes([
            self.data[self.pos],
            self.data[self.pos + 1],
            self.data[self.pos + 2],
            self.data[self.pos + 3],
        ]);
        self.pos += 4;
        Ok(v)
    }

    pub fn read_string(&mut self) -> Result<String, String> {
        self.align(4);
        let len = self.read_u32()? as usize;
        if self.pos + len + 1 > self.data.len() {
            return Err("Unexpected end of data reading string".to_string());
        }
        let s = String::from_utf8_lossy(&self.data[self.pos..self.pos + len]).to_string();
        self.pos += len + 1;
        Ok(s)
    }

    pub fn read_object_path(&mut self) -> Result<String, String> {
        self.read_string()
    }

    pub fn read_signature(&mut self) -> Result<String, String> {
        let len = self.read_byte()? as usize;
        if self.pos + len + 1 > self.data.len() {
            return Err("Unexpected end of data reading signature".to_string());
        }
        let s = String::from_utf8_lossy(&self.data[self.pos..self.pos + len]).to_string();
        self.pos += len + 1;
        Ok(s)
    }

    pub fn read_variant_string(&mut self) -> Result<Option<String>, String> {
        let sig = self.read_signature()?;
        match sig.as_str() {
            "s" | "o" => Ok(Some(self.read_string()?)),
            _ => {
                // Not a string: skip the value by its signature. Erroring here
                // would abort dict scans (e.g. the portal response) whenever a
                // backend adds a key of a type we don't consume.
                self.skip_value(&sig, 0)?;
                Ok(None)
            }
        }
    }

    fn advance(&mut self, n: usize) -> Result<(), String> {
        if self.pos + n > self.data.len() {
            return Err("Unexpected end of data while skipping value".to_string());
        }
        self.pos += n;
        Ok(())
    }

    /// Skip one complete value of the given signature. Bounds-checked so
    /// malformed data errors instead of panicking.
    fn skip_value(&mut self, sig: &str, depth: u8) -> Result<(), String> {
        if depth > 32 {
            return Err("D-Bus signature nesting too deep".to_string());
        }
        let first = *sig.as_bytes().first().ok_or("Empty type signature")?;
        match first {
            b'y' => { self.read_byte()?; }
            b'n' | b'q' => { self.align(2); self.advance(2)?; }
            b'b' | b'i' | b'u' | b'h' => { self.align(4); self.advance(4)?; }
            b'x' | b't' | b'd' => { self.align(8); self.advance(8)?; }
            b's' | b'o' => { self.read_string()?; }
            b'g' => { self.read_signature()?; }
            b'a' => {
                let len = self.read_u32()? as usize;
                self.align(type_alignment(&sig[1..]));
                self.advance(len)?;
            }
            b'v' => {
                let inner = self.read_signature()?;
                self.skip_value(&inner, depth + 1)?;
            }
            b'(' | b'{' => {
                self.align(8);
                let inner = &sig[1..sig.len().saturating_sub(1)];
                let mut rest = inner;
                while !rest.is_empty() {
                    let n = complete_type_len(rest)?;
                    self.skip_value(&rest[..n], depth + 1)?;
                    rest = &rest[n..];
                }
            }
            other => return Err(format!("Unsupported type in signature: {}", other as char)),
        }
        Ok(())
    }
}

/// Alignment of a D-Bus type, from the first character of its signature.
pub(crate) fn type_alignment(sig: &str) -> usize {
    match sig.as_bytes().first() {
        Some(b'y') | Some(b'g') | Some(b'v') => 1,
        Some(b'n') | Some(b'q') => 2,
        Some(b'x') | Some(b't') | Some(b'd') | Some(b'(') | Some(b'{') => 8,
        _ => 4, // b i u h s o a
    }
}

/// Length in bytes of the first complete type in a signature
/// (e.g. "a{sv}u" -> 5, covering "a{sv}").
pub(crate) fn complete_type_len(sig: &str) -> Result<usize, String> {
    let bytes = sig.as_bytes();
    match bytes.first() {
        None => Err("Empty type signature".to_string()),
        Some(b'a') => Ok(1 + complete_type_len(&sig[1..])?),
        Some(open @ (b'(' | b'{')) => {
            let close = if *open == b'(' { b')' } else { b'}' };
            let mut depth = 0usize;
            for (i, &c) in bytes.iter().enumerate() {
                if c == *open { depth += 1; }
                if c == close {
                    depth -= 1;
                    if depth == 0 { return Ok(i + 1); }
                }
            }
            Err(format!("Unbalanced container in signature: {}", sig))
        }
        Some(_) => Ok(1),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_read_variant_skips_unknown_types() {
        // A portal backend may add keys of types we don't consume (doubles,
        // structs) before "uri" — they must be skipped, not fatal.
        let mut buf = MarshalBuffer::new();
        buf.write_signature("d");
        buf.align(8);
        buf.data.extend_from_slice(&1.5f64.to_le_bytes());
        buf.write_string("after");
        let bytes = buf.into_bytes();

        let mut ubuf = UnmarshalBuffer::new(&bytes);
        assert_eq!(ubuf.read_variant_string().unwrap(), None);
        assert_eq!(ubuf.read_string().unwrap(), "after");
    }

    #[test]
    fn test_read_variant_still_reads_strings_and_bools() {
        let mut buf = MarshalBuffer::new();
        buf.write_variant_string("hello");
        buf.write_variant_bool(true);
        buf.write_variant_string("world");
        let bytes = buf.into_bytes();

        let mut ubuf = UnmarshalBuffer::new(&bytes);
        assert_eq!(ubuf.read_variant_string().unwrap(), Some("hello".to_string()));
        assert_eq!(ubuf.read_variant_string().unwrap(), None);
        assert_eq!(ubuf.read_variant_string().unwrap(), Some("world".to_string()));
    }
}
