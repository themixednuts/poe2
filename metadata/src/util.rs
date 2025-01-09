use encoding_rs::mem;

pub fn read_string_from_utf16(src: &[u8]) -> String {
    let mut buffer = vec![0; src.len() * 3];
    let len = mem::convert_utf8_to_utf16(&src, &mut buffer);

    let buffer: Vec<u16> = buffer.chunks(2).skip(1).map(|chunk| chunk[0]).collect();

    let src = String::from_utf16(&buffer).unwrap();
    src.trim_matches('\0').to_string()
}
