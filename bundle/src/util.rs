use std::ffi::CString;

pub enum Either<L, R> {
    Left(L),
    Right(R),
}

pub fn read_bytes<'a>(slice: &'a [u8], n: usize, offset: &mut usize) -> &'a [u8] {
    let val = &slice[*offset..*offset + n];
    *offset += n;
    val
}
pub fn read_u32(slice: &[u8], offset: &mut usize) -> u32 {
    let val = u32::from_le_bytes(slice[*offset..*offset + 4].try_into().unwrap());
    *offset += 4;
    val
}
pub fn read_i32(slice: &[u8], offset: &mut usize) -> i32 {
    let val = i32::from_le_bytes(slice[*offset..*offset + 4].try_into().unwrap());
    *offset += 4;
    val
}

pub fn read_u64(slice: &[u8], offset: &mut usize) -> u64 {
    let val = u64::from_le_bytes(slice[*offset..*offset + 8].try_into().unwrap());
    *offset += 8;
    val
}
pub fn read_i64(slice: &[u8], offset: &mut usize) -> i64 {
    let val = i64::from_le_bytes(slice[*offset..*offset + 8].try_into().unwrap());
    *offset += 8;
    val
}
pub fn find_cstring(slice: &[u8], offset: &mut usize) -> Option<String> {
    let slice = &slice[*offset..];
    let pos = slice.iter().position(|&b| b == 0)?;

    let string = CString::new(&slice[..pos])
        .expect("no null")
        .into_string()
        .unwrap();

    *offset += pos + 1;
    Some(string)
}
