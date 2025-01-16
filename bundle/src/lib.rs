pub mod index;
mod util;

use std::{ffi::c_void, io::Read, marker::PhantomData, sync::Arc};

use oodle_safe::{CompressOptions, BLOCK_LEN};
use oodle_sys::{
    OodleLZSeekTable_Flags_OodleLZSeekTable_Flags_None, OodleLZ_CompressOptions_GetDefault,
    OodleLZ_CompressionLevel_OodleLZ_CompressionLevel_Normal,
    OodleLZ_Compressor_OodleLZ_Compressor_Hydra, OodleLZ_CreateSeekTable,
    OodleLZ_GetCompressedBufferSizeNeeded, OodleLZ_GetSeekTableMemorySizeNeeded, OodleLZ_SeekTable,
};
use rayon::prelude::*;

use crate::util::{read_bytes, read_i32, read_i64, read_u32};

#[derive(Debug, Clone)]
pub struct Bundle<T = Arc<[u8]>> {
    uncompressed_size: u32,
    compressed_size: u32,
    seek_table_size: u32,
    seek_table: OodleLZ_SeekTable,
    seek_chunk_comp_lens: Arc<[u32]>,
    raw_crcs: Option<Arc<[u32]>>,
    chunks: Arc<[Arc<[u8]>]>,
    _marker: PhantomData<T>,
}

impl<T> Bundle<T> {
    pub fn size(&self) -> usize {
        12 + self.seek_table_size as usize + self.compressed_size as usize
    }
}

impl<T> Bundle<T>
where
    T: for<'a> TryFrom<&'a [u8]>,
    for<'a> <T as TryFrom<&'a [u8]>>::Error: std::fmt::Debug,
{
    pub fn from_reader(mut reader: impl Read) -> std::io::Result<Self> {
        let mut buf = vec![];
        reader.read_to_end(&mut buf)?;

        Ok(Self::try_from(buf.as_slice()).unwrap())
    }

    pub fn from_slice(slice: &[u8]) -> std::io::Result<Self> {
        Bundle::try_from(slice)
    }

    pub fn decompress(&self) -> std::io::Result<T> {
        T::try_from(self._decompress()?.as_slice())
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, format!("{:?}", e)))
    }

    fn _decompress(&self) -> std::io::Result<Vec<u8>> {
        let total_size = self.seek_table.totalRawLen as usize;
        let block_size = self.seek_table.seekChunkLen as usize;
        let mut buffer = vec![0; total_size];

        self.chunks
            .par_iter()
            .zip(buffer.par_chunks_mut(block_size))
            .for_each(|(chunk, buf)| {
                let res = oodle_safe::decompress(
                    chunk,
                    buf,
                    None,
                    None,
                    None,
                    Some(oodle_safe::DecodeThreadPhase::All),
                );

                let _bytes = res.unwrap();
            });

        Ok(buffer)
    }
}

impl<T> Bundle<T>
where
    T: Into<Arc<[u8]>>,
{
    pub fn new(data: T) -> Result<Self, ()> {
        let data: Arc<[u8]> = data.into();

        let chunks: Vec<Vec<u8>> = data
            .par_chunks(BLOCK_LEN as usize)
            .enumerate()
            .map(|(i, chunk)| {
                let options = unsafe {
                    let ptr = OodleLZ_CompressOptions_GetDefault(
                        OodleLZ_Compressor_OodleLZ_Compressor_Hydra,
                        OodleLZ_CompressionLevel_OodleLZ_CompressionLevel_Normal,
                    );

                    ptr.as_ref().map(|ptr| *ptr)
                }
                .map(|mut options| {
                    options.seekChunkReset = 1;
                    CompressOptions::from(options)
                });
                let compressed_size = unsafe {
                    OodleLZ_GetCompressedBufferSizeNeeded(
                        OodleLZ_Compressor_OodleLZ_Compressor_Hydra,
                        chunk.len() as isize,
                    )
                };

                // FIXME something is wrong here, why doesnt this compress to the same size
                let mut compressed = vec![0; compressed_size as usize];
                let compressed_size = oodle_safe::compress(
                    oodle_safe::Compressor::Hydra,
                    chunk,
                    &mut compressed,
                    oodle_safe::CompressionLevel::Normal,
                    options,
                    None,
                    None,
                );
                if let Ok(size) = compressed_size {
                    if size < compressed.len() as usize {
                        compressed.resize(size, 0);
                    }
                } else {
                    eprintln!("[Oodle Error] Index: {i} Size: {}", chunk.len());
                }
                compressed
            })
            .collect();

        let compressed: Vec<&u8> = chunks.iter().flatten().collect();

        let seek_table = unsafe {
            let ptr = OodleLZ_CreateSeekTable(
                OodleLZSeekTable_Flags_OodleLZSeekTable_Flags_None,
                BLOCK_LEN as i32,
                data.as_ptr() as *const _,
                data.len() as isize,
                compressed.as_ptr() as *const c_void,
                compressed.len() as isize,
            );

            ptr.as_ref().map(|ptr| *ptr)
        }
        .ok_or(())?;

        let seek_table_size = unsafe {
            OodleLZ_GetSeekTableMemorySizeNeeded(
                data.chunks(BLOCK_LEN as usize).count() as i32,
                OodleLZSeekTable_Flags_OodleLZSeekTable_Flags_None,
            )
        };

        // TODO Errors
        if seek_table.totalRawLen != data.len() as i64 {
            return Err(());
        };

        if seek_table.totalCompLen != compressed.len() as i64 {
            return Err(());
        };

        let seek_chunk_comp_lens = unsafe {
            seek_table.seekChunkCompLens.as_ref().map(|v| {
                std::slice::from_raw_parts(v, seek_table.numSeekChunks as usize)
                    .to_vec()
                    .into()
            })
        }
        .unwrap_or_default();

        let raw_crcs = unsafe {
            seek_table.rawCRCs.as_ref().map(|v| {
                std::slice::from_raw_parts(v, seek_table.numSeekChunks as usize)
                    .to_vec()
                    .into()
            })
        };
        let compressed_size = compressed.len() as u32;

        let chunks: Arc<[Arc<[u8]>]> = chunks
            .into_iter()
            .map(|vec| Arc::from(vec))
            .collect::<Vec<Arc<[u8]>>>()
            .into();

        Ok(Self {
            uncompressed_size: data.len() as u32,
            compressed_size,
            seek_table_size: seek_table_size as u32,
            seek_table,
            seek_chunk_comp_lens,
            raw_crcs,
            chunks,
            _marker: PhantomData,
        })
    }

    pub fn to_vec(self) -> Vec<u8> {
        <Bundle<T> as Into<Vec<u8>>>::into(self)
    }
}

impl<T> TryFrom<&[u8]> for Bundle<T>
where
    T: for<'a> TryFrom<&'a [u8]>,
    for<'a> <T as TryFrom<&'a [u8]>>::Error: std::fmt::Debug,
{
    type Error = std::io::Error;

    fn try_from(value: &[u8]) -> Result<Self, Self::Error> {
        let mut offset = 0;
        let uncompressed_size = read_u32(value, &mut offset);
        let compressed_size = read_u32(value, &mut offset);
        let seek_table_size = read_u32(value, &mut offset);

        let mut seek_table = OodleLZ_SeekTable {
            compressor: read_i32(value, &mut offset),
            seekChunksIndependent: read_i32(value, &mut offset),
            totalRawLen: read_i64(value, &mut offset),
            totalCompLen: read_i64(value, &mut offset),
            numSeekChunks: read_i32(value, &mut offset),
            seekChunkLen: read_i32(value, &mut offset),
            seekChunkCompLens: read_i64(value, &mut offset) as *mut _,
            rawCRCs: read_i64(value, &mut offset) as *mut _,
        };

        let seek_chunk_bytes = read_bytes(
            value,
            seek_table.numSeekChunks as usize * size_of::<u32>(),
            &mut offset,
        );

        let mut seek_chunk_comp_lens: Vec<u32> = seek_chunk_bytes
            .chunks_exact(size_of::<u32>())
            .map(|chunk| u32::from_le_bytes(chunk.try_into().unwrap()))
            .collect();
        seek_table.seekChunkCompLens = seek_chunk_comp_lens.as_mut_ptr();

        let chunks = seek_chunk_comp_lens
            .iter()
            .map(|&size| {
                let chunk = value[offset..offset + size as usize].to_vec();
                offset += size as usize;
                Arc::from(chunk)
            })
            .collect();

        // FIXME read utils need to be Result<_>
        let raw_crcs = if offset != value.len() {
            let raw_crcs_bytes = read_bytes(
                value,
                seek_table.numSeekChunks as usize * size_of::<u32>(),
                &mut offset,
            );

            let mut raw_crcs: Vec<u32> = raw_crcs_bytes
                .chunks_exact(size_of::<u32>())
                .map(|chunk| u32::from_le_bytes(chunk.try_into().unwrap()))
                .collect();

            seek_table.rawCRCs = raw_crcs.as_mut_ptr();
            Some(raw_crcs.into())
        } else {
            None
        };

        Ok(Self {
            uncompressed_size,
            compressed_size,
            seek_table_size,
            seek_table,
            seek_chunk_comp_lens: seek_chunk_comp_lens.into(),
            raw_crcs,
            chunks,
            _marker: PhantomData,
        })
    }
}

impl<T> From<Bundle<T>> for Vec<u8> {
    fn from(value: Bundle<T>) -> Self {
        value.into()
    }
}
impl<T> From<&Bundle<T>> for Vec<u8> {
    fn from(value: &Bundle<T>) -> Self {
        value.into()
    }
}
impl<T> From<Bundle<T>> for Arc<[u8]> {
    fn from(value: Bundle<T>) -> Self {
        value.into()
    }
}
impl<T> From<&Bundle<T>> for Arc<[u8]> {
    fn from(value: &Bundle<T>) -> Self {
        value.into()
    }
}

// FIXME handle drop correctly when not initiated via lib
// impl<T> Drop for Bundle<T> {
//     fn drop(&mut self) {
//         unsafe {
//             oodle_sys::OodleLZ_FreeSeekTable(&mut self.seek_table);
//         };
//     }
// }

#[cfg(test)]
mod tests {
    use std::time::Instant;

    use super::Bundle;
    use crate::index::Index;

    #[test]
    fn read() {
        let index = include_bytes!("../resources/_.index.bin");

        let mut start = Instant::now();
        let i: Bundle<Index> = Bundle::from_slice(index).unwrap();
        println!("[Bundle::from_slice]: {:?}", start.elapsed());

        start = Instant::now();
        let i = i.decompress().unwrap();
        println!("[Bundle::decompress]: {:?}", start.elapsed());

        let bundle: Bundle<Index> = Bundle::new(i.clone()).unwrap();
        let index_2 = bundle.decompress().unwrap();
        let index_2 = <Index as Into<Vec<u8>>>::into(index_2);

        let index_vec_u8 = <&Index as Into<Vec<u8>>>::into(&i);
        assert_eq!(index_vec_u8.len(), index_2.len());
        assert_eq!(index_vec_u8, index_2);

        start = Instant::now();
        let file = std::fs::File::open("D:/Projects/poe2/resources/_.index.bin").unwrap();
        let file_bundle: Bundle<Index> = Bundle::from_reader(file).unwrap();
        println!("[Bundle::from_reader]: {:?}", start.elapsed());

        let file_index = file_bundle.decompress().unwrap().to_vec();
        assert_eq!(index_vec_u8, file_index);
    }
}
