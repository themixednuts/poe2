pub fn read_string_from_utf16(src: &[u8]) -> String {
    let chunks = src.chunks_exact(2);
    assert_eq!(0, chunks.remainder().len());

    let buffer: Vec<u16> = chunks
        .map(|byte| u16::from_le_bytes([byte[0], byte[1]]))
        .collect();

    String::from_utf16(&buffer).unwrap()
}
