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
        while self.data.len() % alignment != 0 {
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
        while data_start < self.data.len() && data_start % 8 != 0 && data_start < len_field_end + 8 {
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
        while self.pos % alignment != 0 && self.pos < self.data.len() {
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
            "b" => { self.read_u32()?; Ok(None) }
            "u" => { self.read_u32()?; Ok(None) }
            _ => {
                Err(format!("Cannot parse variant with signature '{}'", sig))
            }
        }
    }
}
