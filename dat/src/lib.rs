use std::{any::type_name, marker::PhantomData};

const SEPERATOR: [u8; 8] = [0xBB; 8];

pub struct Datc64<'a, T> {
    row_bytes: Vec<&'a [u8]>,
    _marker: PhantomData<T>,
}

impl<T> From<&[u8]> for Datc64<'_, T> {
    fn from(value: &[u8]) -> Self {
        let count = u32::from_le_bytes(value[..4].try_into().unwrap()) as usize;
        let data_offset = value.windows(8).position(|win| win == SEPERATOR);

        let rows_data = data_offset.map_or(&[][..], |i| &value[i..]);
        let row_end_index = data_offset.map_or(value.len(), |i| i);

        let row_length = rows_data.len() / count;
        let mut row_bytes = Vec::with_capacity(row_length);

        for i in 0..count {
            let row_data = &rows_data[(i * row_length)..((i + 1) * row_length)];
            row_bytes.push(row_data);
        }

        assert!(
            count < size_of::<T>(),
            "Row data length {count} does not match struct size {} for {}",
            size_of::<T>(),
            type_name::<T>()
        );
        todo!()
    }
}
