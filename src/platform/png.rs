/// Minimal PNG reader/writer for cropping — zero dependencies.
///
/// Reads PNG files (decompresses IDAT via inflate, undoes row filters),
/// crops a rectangular region, and writes a new PNG with stored (uncompressed) IDAT blocks.

/// Decoded image data.
pub struct Image {
    pub width: u32,
    pub height: u32,
    /// Bytes per pixel (3 = RGB, 4 = RGBA).
    pub bpp: u32,
    /// Row-major pixel data, length = width * height * bpp.
    pub pixels: Vec<u8>,
}

/// Read a PNG file, returning decoded pixel data.
pub fn read_png(path: &str) -> Result<Image, String> {
    let data = std::fs::read(path).map_err(|e| format!("Failed to read {}: {}", path, e))?;
    decode_png(&data)
}

/// Write an Image as a PNG file using stored (uncompressed) IDAT blocks.
pub fn write_png(path: &str, img: &Image) -> Result<(), String> {
    let data = encode_png(img)?;
    std::fs::write(path, &data).map_err(|e| format!("Failed to write {}: {}", path, e))
}

/// Crop a rectangular region from an image. Coordinates are clamped to image bounds.
pub fn crop(img: &Image, x: u32, y: u32, w: u32, h: u32) -> Result<Image, String> {
    // Clamp to image bounds
    let x = x.min(img.width);
    let y = y.min(img.height);
    let w = w.min(img.width.saturating_sub(x));
    let h = h.min(img.height.saturating_sub(y));

    if w == 0 || h == 0 {
        return Err("Crop region is empty after clamping".to_string());
    }

    let bpp = img.bpp as usize;
    let src_stride = img.width as usize * bpp;
    let dst_stride = w as usize * bpp;
    let mut pixels = vec![0u8; h as usize * dst_stride];

    for row in 0..h as usize {
        let src_off = (y as usize + row) * src_stride + x as usize * bpp;
        let dst_off = row * dst_stride;
        pixels[dst_off..dst_off + dst_stride]
            .copy_from_slice(&img.pixels[src_off..src_off + dst_stride]);
    }

    Ok(Image { width: w, height: h, bpp: img.bpp, pixels })
}

// ---------------------------------------------------------------------------
// PNG decoding
// ---------------------------------------------------------------------------

fn decode_png(data: &[u8]) -> Result<Image, String> {
    // Check PNG signature
    if data.len() < 8 || &data[0..8] != b"\x89PNG\r\n\x1a\n" {
        return Err("Not a valid PNG file".to_string());
    }

    let mut pos = 8;
    let mut width = 0u32;
    let mut height = 0u32;
    let mut bit_depth: u8;
    let mut color_type = 0u8;
    let mut idat_chunks: Vec<&[u8]> = Vec::new();

    while pos + 8 <= data.len() {
        let chunk_len = u32::from_be_bytes([data[pos], data[pos+1], data[pos+2], data[pos+3]]) as usize;
        let chunk_type = &data[pos+4..pos+8];
        let chunk_data_start = pos + 8;
        let chunk_data_end = chunk_data_start + chunk_len;

        if chunk_data_end + 4 > data.len() {
            return Err("PNG chunk extends past end of file".to_string());
        }

        match chunk_type {
            b"IHDR" => {
                if chunk_len < 13 {
                    return Err("IHDR chunk too short".to_string());
                }
                let d = &data[chunk_data_start..];
                width = u32::from_be_bytes([d[0], d[1], d[2], d[3]]);
                height = u32::from_be_bytes([d[4], d[5], d[6], d[7]]);
                bit_depth = d[8];
                color_type = d[9];
                let compression = d[10];
                let filter = d[11];
                let interlace = d[12];
                if compression != 0 { return Err("Unsupported compression method".to_string()); }
                if filter != 0 { return Err("Unsupported filter method".to_string()); }
                if interlace != 0 { return Err("Interlaced PNGs not supported".to_string()); }
                if bit_depth != 8 { return Err(format!("Only 8-bit depth supported, got {}", bit_depth)); }
                if color_type != 2 && color_type != 6 {
                    return Err(format!("Only RGB(2) and RGBA(6) color types supported, got {}", color_type));
                }
            }
            b"IDAT" => {
                idat_chunks.push(&data[chunk_data_start..chunk_data_end]);
            }
            b"IEND" => break,
            _ => {} // skip ancillary chunks
        }

        pos = chunk_data_end + 4; // skip CRC
    }

    if width == 0 || height == 0 {
        return Err("Missing IHDR chunk".to_string());
    }

    // Concatenate all IDAT data
    let total_idat: usize = idat_chunks.iter().map(|c| c.len()).sum();
    let mut idat_data = Vec::with_capacity(total_idat);
    for chunk in &idat_chunks {
        idat_data.extend_from_slice(chunk);
    }

    // Decompress zlib stream
    let raw = zlib_decompress(&idat_data)?;

    // Unfilter
    let bpp: u32 = if color_type == 2 { 3 } else { 4 };
    let stride = width as usize * bpp as usize;
    let expected = height as usize * (1 + stride); // 1 byte filter per row
    if raw.len() < expected {
        return Err(format!("Decompressed data too short: {} < {}", raw.len(), expected));
    }

    let mut pixels = vec![0u8; height as usize * stride];
    for y in 0..height as usize {
        let row_start = y * (1 + stride);
        let filter_byte = raw[row_start];
        let row_data = &raw[row_start + 1..row_start + 1 + stride];
        let dst_off = y * stride;

        match filter_byte {
            0 => {
                // None
                pixels[dst_off..dst_off + stride].copy_from_slice(row_data);
            }
            1 => {
                // Sub
                for i in 0..stride {
                    let a = if i >= bpp as usize { pixels[dst_off + i - bpp as usize] } else { 0 };
                    pixels[dst_off + i] = row_data[i].wrapping_add(a);
                }
            }
            2 => {
                // Up
                for i in 0..stride {
                    let b = if y > 0 { pixels[dst_off - stride + i] } else { 0 };
                    pixels[dst_off + i] = row_data[i].wrapping_add(b);
                }
            }
            3 => {
                // Average
                for i in 0..stride {
                    let a = if i >= bpp as usize { pixels[dst_off + i - bpp as usize] as u16 } else { 0 };
                    let b = if y > 0 { pixels[dst_off - stride + i] as u16 } else { 0 };
                    pixels[dst_off + i] = row_data[i].wrapping_add(((a + b) / 2) as u8);
                }
            }
            4 => {
                // Paeth
                for i in 0..stride {
                    let a = if i >= bpp as usize { pixels[dst_off + i - bpp as usize] as i32 } else { 0 };
                    let b = if y > 0 { pixels[dst_off - stride + i] as i32 } else { 0 };
                    let c = if y > 0 && i >= bpp as usize { pixels[dst_off - stride + i - bpp as usize] as i32 } else { 0 };
                    let p = a + b - c;
                    let pa = (p - a).abs();
                    let pb = (p - b).abs();
                    let pc = (p - c).abs();
                    let pr = if pa <= pb && pa <= pc { a } else if pb <= pc { b } else { c };
                    pixels[dst_off + i] = row_data[i].wrapping_add(pr as u8);
                }
            }
            _ => return Err(format!("Unknown PNG filter type: {}", filter_byte)),
        }
    }

    Ok(Image { width, height, bpp, pixels })
}

// ---------------------------------------------------------------------------
// Zlib / Inflate decompression (RFC 1950 / RFC 1951)
// ---------------------------------------------------------------------------

fn zlib_decompress(data: &[u8]) -> Result<Vec<u8>, String> {
    if data.len() < 6 {
        return Err("Zlib data too short".to_string());
    }
    // Skip 2-byte zlib header
    let cmf = data[0];
    let cm = cmf & 0x0F;
    if cm != 8 {
        return Err(format!("Unsupported zlib compression method: {}", cm));
    }
    // Ignore checksum at end (4 bytes)
    inflate(&data[2..data.len() - 4])
}

/// Bit reader for the inflate stream.
struct BitReader<'a> {
    data: &'a [u8],
    pos: usize,      // byte position
    bit_buf: u32,
    bits_in: u8,
}

impl<'a> BitReader<'a> {
    fn new(data: &'a [u8]) -> Self {
        BitReader { data, pos: 0, bit_buf: 0, bits_in: 0 }
    }

    fn ensure_bits(&mut self, n: u8) {
        while self.bits_in < n {
            let byte = if self.pos < self.data.len() { self.data[self.pos] } else { 0 };
            self.pos += 1;
            self.bit_buf |= (byte as u32) << self.bits_in;
            self.bits_in += 8;
        }
    }

    fn read_bits(&mut self, n: u8) -> u32 {
        self.ensure_bits(n);
        let val = self.bit_buf & ((1u32 << n) - 1);
        self.bit_buf >>= n;
        self.bits_in -= n;
        val
    }

    #[allow(dead_code)]
    fn read_bits_rev(&mut self, n: u8) -> u32 {
        // Read n bits and reverse them (for Huffman codes which are MSB-first)
        let val = self.read_bits(n);
        reverse_bits(val, n)
    }

    /// Align to next byte boundary (discard remaining bits in current byte).
    fn align_byte(&mut self) {
        let discard = self.bits_in % 8;
        if discard > 0 {
            self.bit_buf >>= discard;
            self.bits_in -= discard;
        }
    }

    fn read_byte(&mut self) -> u8 {
        self.align_byte();
        self.read_bits(8) as u8
    }

    fn read_u16_le(&mut self) -> u16 {
        let lo = self.read_byte() as u16;
        let hi = self.read_byte() as u16;
        lo | (hi << 8)
    }
}

fn reverse_bits(val: u32, n: u8) -> u32 {
    let mut result = 0u32;
    let mut v = val;
    for _ in 0..n {
        result = (result << 1) | (v & 1);
        v >>= 1;
    }
    result
}

/// Huffman decoder using lookup table.
struct HuffmanTable {
    /// For codes up to MAX_BITS, store (symbol, code_length) indexed by reversed code.
    /// We use a flat lookup table for codes up to 15 bits.
    /// Entry: symbol in low 16 bits, length in high 16 bits. 0 = invalid.
    table: Vec<u32>,
    max_bits: u8,
}

const MAX_HUFFMAN_BITS: u8 = 15;

impl HuffmanTable {
    fn from_lengths(lengths: &[u8]) -> Result<HuffmanTable, String> {
        // Count codes of each length
        let max_bits = *lengths.iter().max().unwrap_or(&0);
        if max_bits == 0 {
            return Ok(HuffmanTable { table: vec![0; 1], max_bits: 0 });
        }
        let max_bits = max_bits.min(MAX_HUFFMAN_BITS);

        let mut bl_count = [0u32; 16];
        for &len in lengths {
            if len > 0 {
                bl_count[len as usize] += 1;
            }
        }

        // Compute first code for each length
        let mut next_code = [0u32; 16];
        let mut code = 0u32;
        for bits in 1..=max_bits as usize {
            code = (code + bl_count[bits - 1]) << 1;
            next_code[bits] = code;
        }

        // Build lookup table
        let table_size = 1usize << max_bits;
        let mut table = vec![0u32; table_size];

        for (sym, &len) in lengths.iter().enumerate() {
            if len == 0 { continue; }
            let len = len as usize;
            let code = next_code[len];
            next_code[len] += 1;

            // Reverse code bits for the table (we read bits LSB-first)
            let rev = reverse_bits(code, len as u8);

            // Fill all table entries where the low `len` bits match `rev`
            let step = 1usize << len;
            let mut idx = rev as usize;
            while idx < table_size {
                table[idx] = (sym as u32) | ((len as u32) << 16);
                idx += step;
            }
        }

        Ok(HuffmanTable { table, max_bits })
    }

    fn decode(&self, reader: &mut BitReader) -> Result<u32, String> {
        reader.ensure_bits(self.max_bits);
        let idx = (reader.bit_buf & ((1u32 << self.max_bits) - 1)) as usize;
        let entry = self.table[idx];
        let len = entry >> 16;
        if len == 0 {
            return Err("Invalid Huffman code".to_string());
        }
        let sym = entry & 0xFFFF;
        reader.bit_buf >>= len;
        reader.bits_in -= len as u8;
        Ok(sym)
    }
}

// Length and distance extra bits tables for deflate
static LENGTH_BASE: [u16; 29] = [
    3, 4, 5, 6, 7, 8, 9, 10, 11, 13,
    15, 17, 19, 23, 27, 31, 35, 43, 51, 59,
    67, 83, 99, 115, 131, 163, 195, 227, 258,
];

static LENGTH_EXTRA: [u8; 29] = [
    0, 0, 0, 0, 0, 0, 0, 0, 1, 1,
    1, 1, 2, 2, 2, 2, 3, 3, 3, 3,
    4, 4, 4, 4, 5, 5, 5, 5, 0,
];

static DIST_BASE: [u16; 30] = [
    1, 2, 3, 4, 5, 7, 9, 13, 17, 25,
    33, 49, 65, 97, 129, 193, 257, 385, 513, 769,
    1025, 1537, 2049, 3073, 4097, 6145, 8193, 12289, 16385, 24577,
];

static DIST_EXTRA: [u8; 30] = [
    0, 0, 0, 0, 1, 1, 2, 2, 3, 3,
    4, 4, 5, 5, 6, 6, 7, 7, 8, 8,
    9, 9, 10, 10, 11, 11, 12, 12, 13, 13,
];

fn inflate(data: &[u8]) -> Result<Vec<u8>, String> {
    let mut reader = BitReader::new(data);
    let mut output = Vec::new();

    loop {
        let bfinal = reader.read_bits(1);
        let btype = reader.read_bits(2);

        match btype {
            0 => {
                // Stored block
                reader.align_byte();
                let len = reader.read_u16_le();
                let _nlen = reader.read_u16_le();
                for _ in 0..len {
                    output.push(reader.read_byte());
                }
            }
            1 => {
                // Fixed Huffman
                inflate_block_huffman(&mut reader, &mut output, true)?;
            }
            2 => {
                // Dynamic Huffman
                inflate_block_huffman(&mut reader, &mut output, false)?;
            }
            3 => return Err("Invalid deflate block type 3".to_string()),
            _ => unreachable!(),
        }

        if bfinal != 0 {
            break;
        }
    }

    Ok(output)
}

fn build_fixed_lit_table() -> HuffmanTable {
    let mut lengths = [0u8; 288];
    for i in 0..=143 { lengths[i] = 8; }
    for i in 144..=255 { lengths[i] = 9; }
    for i in 256..=279 { lengths[i] = 7; }
    for i in 280..=287 { lengths[i] = 8; }
    HuffmanTable::from_lengths(&lengths).unwrap()
}

fn build_fixed_dist_table() -> HuffmanTable {
    let lengths = [5u8; 32];
    HuffmanTable::from_lengths(&lengths).unwrap()
}

fn inflate_block_huffman(reader: &mut BitReader, output: &mut Vec<u8>, fixed: bool) -> Result<(), String> {
    let (lit_table, dist_table) = if fixed {
        (build_fixed_lit_table(), build_fixed_dist_table())
    } else {
        build_dynamic_tables(reader)?
    };

    loop {
        let sym = lit_table.decode(reader)?;

        if sym < 256 {
            output.push(sym as u8);
        } else if sym == 256 {
            break; // end of block
        } else {
            // Length-distance pair
            let len_idx = (sym - 257) as usize;
            if len_idx >= LENGTH_BASE.len() {
                return Err(format!("Invalid length code: {}", sym));
            }
            let length = LENGTH_BASE[len_idx] as usize
                + reader.read_bits(LENGTH_EXTRA[len_idx]) as usize;

            let dist_sym = dist_table.decode(reader)? as usize;
            if dist_sym >= DIST_BASE.len() {
                return Err(format!("Invalid distance code: {}", dist_sym));
            }
            let distance = DIST_BASE[dist_sym] as usize
                + reader.read_bits(DIST_EXTRA[dist_sym]) as usize;

            if distance > output.len() {
                return Err(format!("Distance {} exceeds output size {}", distance, output.len()));
            }

            let start = output.len() - distance;
            for i in 0..length {
                let byte = output[start + (i % distance)];
                output.push(byte);
            }
        }
    }

    Ok(())
}

/// Code length alphabet order for dynamic Huffman tables.
static CODELEN_ORDER: [usize; 19] = [
    16, 17, 18, 0, 8, 7, 9, 6, 10, 5, 11, 4, 12, 3, 13, 2, 14, 1, 15,
];

fn build_dynamic_tables(reader: &mut BitReader) -> Result<(HuffmanTable, HuffmanTable), String> {
    let hlit = reader.read_bits(5) as usize + 257;
    let hdist = reader.read_bits(5) as usize + 1;
    let hclen = reader.read_bits(4) as usize + 4;

    // Read code length code lengths
    let mut codelen_lengths = [0u8; 19];
    for i in 0..hclen {
        codelen_lengths[CODELEN_ORDER[i]] = reader.read_bits(3) as u8;
    }

    let codelen_table = HuffmanTable::from_lengths(&codelen_lengths)?;

    // Decode literal/length + distance code lengths
    let total = hlit + hdist;
    let mut lengths = Vec::with_capacity(total);

    while lengths.len() < total {
        let sym = codelen_table.decode(reader)?;
        match sym {
            0..=15 => lengths.push(sym as u8),
            16 => {
                let repeat = reader.read_bits(2) as usize + 3;
                let last = *lengths.last().ok_or("Code length 16 with no previous")?;
                for _ in 0..repeat { lengths.push(last); }
            }
            17 => {
                let repeat = reader.read_bits(3) as usize + 3;
                for _ in 0..repeat { lengths.push(0); }
            }
            18 => {
                let repeat = reader.read_bits(7) as usize + 11;
                for _ in 0..repeat { lengths.push(0); }
            }
            _ => return Err(format!("Invalid code length symbol: {}", sym)),
        }
    }

    let lit_table = HuffmanTable::from_lengths(&lengths[..hlit])?;
    let dist_table = HuffmanTable::from_lengths(&lengths[hlit..hlit + hdist])?;

    Ok((lit_table, dist_table))
}

// ---------------------------------------------------------------------------
// PNG encoding (stored / uncompressed IDAT)
// ---------------------------------------------------------------------------

fn encode_png(img: &Image) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();

    // PNG signature
    out.extend_from_slice(b"\x89PNG\r\n\x1a\n");

    // IHDR
    let color_type: u8 = if img.bpp == 4 { 6 } else { 2 }; // RGBA or RGB
    let mut ihdr = Vec::with_capacity(13);
    ihdr.extend_from_slice(&img.width.to_be_bytes());
    ihdr.extend_from_slice(&img.height.to_be_bytes());
    ihdr.push(8); // bit depth
    ihdr.push(color_type);
    ihdr.push(0); // compression
    ihdr.push(0); // filter
    ihdr.push(0); // interlace
    write_chunk(&mut out, b"IHDR", &ihdr);

    // Build raw filtered data (filter type 0 = None for each row)
    let stride = img.width as usize * img.bpp as usize;
    let raw_len = img.height as usize * (1 + stride);
    let mut raw = Vec::with_capacity(raw_len);
    for y in 0..img.height as usize {
        raw.push(0); // filter byte: None
        let row_start = y * stride;
        raw.extend_from_slice(&img.pixels[row_start..row_start + stride]);
    }

    // Wrap in zlib with stored deflate blocks
    let idat_data = zlib_stored(&raw);
    write_chunk(&mut out, b"IDAT", &idat_data);

    // IEND
    write_chunk(&mut out, b"IEND", &[]);

    Ok(out)
}

fn write_chunk(out: &mut Vec<u8>, chunk_type: &[u8; 4], data: &[u8]) {
    out.extend_from_slice(&(data.len() as u32).to_be_bytes());
    out.extend_from_slice(chunk_type);
    out.extend_from_slice(data);
    let crc = crc32(&[chunk_type.as_slice(), data].concat());
    out.extend_from_slice(&crc.to_be_bytes());
}

/// Produce a zlib stream using stored (no compression) deflate blocks.
fn zlib_stored(data: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    // Zlib header: CM=8 (deflate), CINFO=7 (32K window)
    // CMF = 0x78, FLG = 0x01 (FCHECK so (CMF*256+FLG) % 31 == 0)
    out.push(0x78);
    out.push(0x01);

    // Split into stored blocks of max 65535 bytes
    let max_block = 65535usize;
    let mut offset = 0;
    while offset < data.len() {
        let remaining = data.len() - offset;
        let block_len = remaining.min(max_block);
        let is_final = offset + block_len >= data.len();

        out.push(if is_final { 0x01 } else { 0x00 }); // BFINAL + BTYPE=00
        let len = block_len as u16;
        let nlen = !len;
        out.push(len as u8);
        out.push((len >> 8) as u8);
        out.push(nlen as u8);
        out.push((nlen >> 8) as u8);
        out.extend_from_slice(&data[offset..offset + block_len]);
        offset += block_len;
    }

    // Handle empty data
    if data.is_empty() {
        out.push(0x01); // BFINAL=1, BTYPE=00
        out.push(0x00);
        out.push(0x00);
        out.push(0xFF);
        out.push(0xFF);
    }

    // Adler-32 checksum
    let checksum = adler32(data);
    out.extend_from_slice(&checksum.to_be_bytes());

    out
}

fn adler32(data: &[u8]) -> u32 {
    let mut a: u32 = 1;
    let mut b: u32 = 0;
    for &byte in data {
        a = (a + byte as u32) % 65521;
        b = (b + a) % 65521;
    }
    (b << 16) | a
}

/// CRC-32 as specified by PNG (ISO 3309 / ITU-T V.42).
fn crc32(data: &[u8]) -> u32 {
    // Build table on the stack
    let mut table = [0u32; 256];
    for n in 0..256u32 {
        let mut c = n;
        for _ in 0..8 {
            if c & 1 != 0 {
                c = 0xEDB88320 ^ (c >> 1);
            } else {
                c >>= 1;
            }
        }
        table[n as usize] = c;
    }

    let mut crc = 0xFFFFFFFFu32;
    for &byte in data {
        let idx = ((crc ^ byte as u32) & 0xFF) as usize;
        crc = table[idx] ^ (crc >> 8);
    }
    crc ^ 0xFFFFFFFF
}

// ---------------------------------------------------------------------------
// Bitmap font and grid overlay
// ---------------------------------------------------------------------------

/// 6x8 bitmap font glyphs for grid labels. Each glyph is 8 bytes (one per row).
/// Bits are MSB-first, 6 bits per row.
#[allow(dead_code)]
const FONT_GLYPHS: &[(char, [u8; 8])] = &[
    ('A', [0b00110000, 0b01001000, 0b10000100, 0b10000100, 0b11111100, 0b10000100, 0b10000100, 0b00000000]),
    ('B', [0b11111000, 0b10000100, 0b10000100, 0b11111000, 0b10000100, 0b10000100, 0b11111000, 0b00000000]),
    ('C', [0b01111000, 0b10000100, 0b10000000, 0b10000000, 0b10000000, 0b10000100, 0b01111000, 0b00000000]),
    ('D', [0b11110000, 0b10001000, 0b10000100, 0b10000100, 0b10000100, 0b10001000, 0b11110000, 0b00000000]),
    ('E', [0b11111100, 0b10000000, 0b10000000, 0b11111000, 0b10000000, 0b10000000, 0b11111100, 0b00000000]),
    ('F', [0b11111100, 0b10000000, 0b10000000, 0b11111000, 0b10000000, 0b10000000, 0b10000000, 0b00000000]),
    ('G', [0b01111000, 0b10000100, 0b10000000, 0b10011100, 0b10000100, 0b10000100, 0b01111000, 0b00000000]),
    ('H', [0b10000100, 0b10000100, 0b10000100, 0b11111100, 0b10000100, 0b10000100, 0b10000100, 0b00000000]),
    ('I', [0b01111000, 0b00110000, 0b00110000, 0b00110000, 0b00110000, 0b00110000, 0b01111000, 0b00000000]),
    ('J', [0b00111100, 0b00001000, 0b00001000, 0b00001000, 0b00001000, 0b10001000, 0b01110000, 0b00000000]),
    ('K', [0b10000100, 0b10001000, 0b10010000, 0b11100000, 0b10010000, 0b10001000, 0b10000100, 0b00000000]),
    ('L', [0b10000000, 0b10000000, 0b10000000, 0b10000000, 0b10000000, 0b10000000, 0b11111100, 0b00000000]),
    ('M', [0b10000100, 0b11001100, 0b10110100, 0b10000100, 0b10000100, 0b10000100, 0b10000100, 0b00000000]),
    ('N', [0b10000100, 0b11000100, 0b10100100, 0b10010100, 0b10001100, 0b10000100, 0b10000100, 0b00000000]),
    ('O', [0b01111000, 0b10000100, 0b10000100, 0b10000100, 0b10000100, 0b10000100, 0b01111000, 0b00000000]),
    ('P', [0b11111000, 0b10000100, 0b10000100, 0b11111000, 0b10000000, 0b10000000, 0b10000000, 0b00000000]),
    ('Q', [0b01111000, 0b10000100, 0b10000100, 0b10000100, 0b10010100, 0b10001000, 0b01110100, 0b00000000]),
    ('R', [0b11111000, 0b10000100, 0b10000100, 0b11111000, 0b10010000, 0b10001000, 0b10000100, 0b00000000]),
    ('S', [0b01111000, 0b10000100, 0b10000000, 0b01111000, 0b00000100, 0b10000100, 0b01111000, 0b00000000]),
    ('T', [0b11111100, 0b00110000, 0b00110000, 0b00110000, 0b00110000, 0b00110000, 0b00110000, 0b00000000]),
    ('U', [0b10000100, 0b10000100, 0b10000100, 0b10000100, 0b10000100, 0b10000100, 0b01111000, 0b00000000]),
    ('V', [0b10000100, 0b10000100, 0b10000100, 0b10000100, 0b01001000, 0b00110000, 0b00110000, 0b00000000]),
    ('W', [0b10000100, 0b10000100, 0b10000100, 0b10000100, 0b10110100, 0b11001100, 0b10000100, 0b00000000]),
    ('X', [0b10000100, 0b01001000, 0b00110000, 0b00110000, 0b00110000, 0b01001000, 0b10000100, 0b00000000]),
    ('Y', [0b10000100, 0b01001000, 0b00110000, 0b00110000, 0b00110000, 0b00110000, 0b00110000, 0b00000000]),
    ('Z', [0b11111100, 0b00001000, 0b00010000, 0b00110000, 0b01000000, 0b10000000, 0b11111100, 0b00000000]),
    ('0', [0b01111000, 0b10001100, 0b10010100, 0b10100100, 0b11000100, 0b10000100, 0b01111000, 0b00000000]),
    ('1', [0b00110000, 0b01110000, 0b00110000, 0b00110000, 0b00110000, 0b00110000, 0b11111100, 0b00000000]),
    ('2', [0b01111000, 0b10000100, 0b00000100, 0b00111000, 0b01000000, 0b10000000, 0b11111100, 0b00000000]),
    ('3', [0b01111000, 0b10000100, 0b00000100, 0b00111000, 0b00000100, 0b10000100, 0b01111000, 0b00000000]),
    ('4', [0b00001000, 0b00011000, 0b00101000, 0b01001000, 0b11111100, 0b00001000, 0b00001000, 0b00000000]),
    ('5', [0b11111100, 0b10000000, 0b11111000, 0b00000100, 0b00000100, 0b10000100, 0b01111000, 0b00000000]),
    ('6', [0b01111000, 0b10000000, 0b10000000, 0b11111000, 0b10000100, 0b10000100, 0b01111000, 0b00000000]),
    ('7', [0b11111100, 0b00000100, 0b00001000, 0b00010000, 0b00100000, 0b00100000, 0b00100000, 0b00000000]),
    ('8', [0b01111000, 0b10000100, 0b10000100, 0b01111000, 0b10000100, 0b10000100, 0b01111000, 0b00000000]),
    ('9', [0b01111000, 0b10000100, 0b10000100, 0b01111100, 0b00000100, 0b00000100, 0b01111000, 0b00000000]),
];

#[allow(dead_code)]
const GLYPH_WIDTH: u32 = 6;
#[allow(dead_code)]
const GLYPH_HEIGHT: u32 = 8;
#[allow(dead_code)]
const LABEL_PADDING: u32 = 3;

#[allow(dead_code)]
fn get_glyph(c: char) -> Option<&'static [u8; 8]> {
    FONT_GLYPHS.iter().find(|(ch, _)| *ch == c).map(|(_, g)| g)
}

/// Draw a labeled grid overlay on an image.
/// Columns are labeled A, B, C...; rows are labeled 1, 2, 3...
/// Label scale adapts to cell size: 2x for large cells, 1x for small cells.
#[allow(dead_code)]
pub fn draw_grid(img: &mut Image, cols: u32, rows: u32) {
    let w = img.width;
    let h = img.height;
    let cell_w = w / cols;
    let cell_h = h / rows;

    // Scale labels based on cell size — use 2x if cells are large enough, else 1x
    let min_cell = cell_w.min(cell_h);
    let scale = if min_cell >= 60 { 2u32 } else { 1u32 };
    let pad = if scale == 2 { 5u32 } else { 2u32 };

    // Draw vertical grid lines
    for col in 1..cols {
        let x = col * cell_w;
        draw_vertical_line(img, x, 0, h);
    }

    // Draw horizontal grid lines
    for row in 1..rows {
        let y = row * cell_h;
        draw_horizontal_line(img, 0, w, y);
    }

    // Draw labels
    for row in 0..rows {
        for col in 0..cols {
            let label_col = (b'A' + col as u8) as char;
            let label_row_char = (b'1' + row as u8) as char;

            let lx = col * cell_w + pad;
            let ly = row * cell_h + pad;

            // Draw background rectangle behind label
            let bg_w = GLYPH_WIDTH * scale * 2 + pad * 2;
            let bg_h = GLYPH_HEIGHT * scale + pad;
            draw_filled_rect(img, lx, ly, bg_w, bg_h, [0, 0, 0, 200]);

            // Draw the two characters (e.g., "A1")
            draw_char_scaled(img, label_col, lx + pad, ly + pad / 2, scale);
            draw_char_scaled(img, label_row_char, lx + pad + GLYPH_WIDTH * scale + 1, ly + pad / 2, scale);
        }
    }
}

#[allow(dead_code)]
fn draw_vertical_line(img: &mut Image, x: u32, y_start: u32, y_end: u32) {
    for y in y_start..y_end.min(img.height) {
        // Black border
        for dx in [0i32, 2] {
            let px = (x as i32 + dx) as u32;
            if px < img.width {
                set_pixel(img, px, y, [0, 0, 0, 255]);
            }
        }
        // White center
        if x + 1 < img.width {
            set_pixel(img, x + 1, y, [255, 255, 255, 255]);
        }
    }
}

#[allow(dead_code)]
fn draw_horizontal_line(img: &mut Image, x_start: u32, x_end: u32, y: u32) {
    for x in x_start..x_end.min(img.width) {
        for dy in [0i32, 2] {
            let py = (y as i32 + dy) as u32;
            if py < img.height {
                set_pixel(img, x, py, [0, 0, 0, 255]);
            }
        }
        if y + 1 < img.height {
            set_pixel(img, x, y + 1, [255, 255, 255, 255]);
        }
    }
}

#[allow(dead_code)]
fn draw_filled_rect(img: &mut Image, x: u32, y: u32, w: u32, h: u32, color: [u8; 4]) {
    for dy in 0..h {
        for dx in 0..w {
            let px = x + dx;
            let py = y + dy;
            if px < img.width && py < img.height {
                set_pixel(img, px, py, color);
            }
        }
    }
}

#[allow(dead_code)]
fn draw_char_scaled(img: &mut Image, c: char, x: u32, y: u32, scale: u32) {
    let glyph = match get_glyph(c) {
        Some(g) => g,
        None => return,
    };
    for row in 0..GLYPH_HEIGHT {
        let bits = glyph[row as usize];
        for col in 0..GLYPH_WIDTH {
            if bits & (0x80 >> col) != 0 {
                for sy in 0..scale {
                    for sx in 0..scale {
                        let px = x + col * scale + sx;
                        let py = y + row * scale + sy;
                        if px < img.width && py < img.height {
                            set_pixel(img, px, py, [255, 255, 255, 255]);
                        }
                    }
                }
            }
        }
    }
}

#[allow(dead_code)]
fn set_pixel(img: &mut Image, x: u32, y: u32, color: [u8; 4]) {
    let bpp = img.bpp as usize;
    let idx = (y * img.width * img.bpp + x * img.bpp) as usize;
    if idx + bpp <= img.pixels.len() {
        img.pixels[idx] = color[0];
        img.pixels[idx + 1] = color[1];
        img.pixels[idx + 2] = color[2];
        if bpp == 4 {
            img.pixels[idx + 3] = color[3];
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_adler32() {
        // Known value: adler32("Wikipedia") = 0x11E60398
        let data = b"Wikipedia";
        assert_eq!(adler32(data), 0x11E60398);
    }

    #[test]
    fn test_crc32() {
        // CRC-32 of "123456789" = 0xCBF43926
        let data = b"123456789";
        assert_eq!(crc32(data), 0xCBF43926);
    }

    #[test]
    fn test_roundtrip_small_image() {
        // Create a tiny 2x2 RGB image and verify roundtrip
        let img = Image {
            width: 2,
            height: 2,
            bpp: 3,
            pixels: vec![
                255, 0, 0,   0, 255, 0,    // row 0: red, green
                0, 0, 255,   255, 255, 0,  // row 1: blue, yellow
            ],
        };

        let encoded = encode_png(&img).unwrap();
        let decoded = decode_png(&encoded).unwrap();

        assert_eq!(decoded.width, 2);
        assert_eq!(decoded.height, 2);
        assert_eq!(decoded.bpp, 3);
        assert_eq!(decoded.pixels, img.pixels);
    }

    #[test]
    fn test_crop() {
        let img = Image {
            width: 4,
            height: 4,
            bpp: 3,
            pixels: vec![0u8; 4 * 4 * 3],
        };
        let cropped = crop(&img, 1, 1, 2, 2).unwrap();
        assert_eq!(cropped.width, 2);
        assert_eq!(cropped.height, 2);
        assert_eq!(cropped.pixels.len(), 2 * 2 * 3);
    }

    #[test]
    fn test_crop_clamp() {
        let img = Image {
            width: 4,
            height: 4,
            bpp: 3,
            pixels: vec![0u8; 4 * 4 * 3],
        };
        // Request extends past image bounds — should clamp
        let cropped = crop(&img, 3, 3, 10, 10).unwrap();
        assert_eq!(cropped.width, 1);
        assert_eq!(cropped.height, 1);
    }

    #[test]
    fn test_inflate_stored() {
        // Manually create a zlib stored block: header + stored block with "hello"
        let input = b"hello";
        let zlib = zlib_stored(input);
        let result = zlib_decompress(&zlib).unwrap();
        assert_eq!(result, input);
    }

    #[test]
    fn test_draw_grid_no_panic() {
        let mut img = Image {
            width: 120,
            height: 80,
            bpp: 3,
            pixels: vec![128u8; 120 * 80 * 3],
        };
        draw_grid(&mut img, 4, 3);
        // Verify the image dimensions are unchanged
        assert_eq!(img.width, 120);
        assert_eq!(img.height, 80);
        assert_eq!(img.pixels.len(), 120 * 80 * 3);
        // Verify grid lines were drawn (center of first vertical line should be white)
        // Check at y=40 to avoid the label background rectangle which covers top-left area
        let line_x = 30u32; // 120/4 = 30
        let idx = (40 * 120 * 3 + (line_x + 1) * 3) as usize;
        assert_eq!(img.pixels[idx], 255); // white line center
    }

    #[test]
    fn test_draw_grid_rgba() {
        let mut img = Image {
            width: 160,
            height: 120,
            bpp: 4,
            pixels: vec![128u8; 160 * 120 * 4],
        };
        draw_grid(&mut img, 4, 3);
        assert_eq!(img.pixels.len(), 160 * 120 * 4);
    }
}
