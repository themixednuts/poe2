use bytemuck::{Pod, Zeroable};
use globset::{Glob, GlobSetBuilder};
use rayon::prelude::*;
use std::{
    collections::HashMap,
    io,
    path::{Path, PathBuf},
    sync::{Arc, OnceLock},
};

use super::{
    util::{find_cstring, read_u32},
    Bundle,
};

#[cfg(feature = "tracing")]
use tracing::{error, info, trace, warn};

#[derive(Debug, Clone)]
pub struct Index<'a> {
    /// List of paths to a Bundle.bin file
    bundles: Arc<[BundleRecord]>,
    files: Arc<[FileRecord]>,
    paths: Arc<[PathRecord]>,
    path_bundle: Bundle<Arc<[u8]>>,
    cache: OnceLock<HashMap<usize, Arc<[(PathBuf, &'a FileRecord)]>>>,
}

impl<'a> Index<'a> {
    pub fn to_vec(self) -> Vec<u8> {
        <Index<'a> as Into<Vec<u8>>>::into(self)
    }

    pub fn total_files(&self) -> usize {
        self.files.len()
    }

    pub fn total_uncompressed_size(&self) -> usize {
        self.files.iter().map(|file| file.size as usize).sum()
    }

    // pub fn list_bundles(&'a self) -> Vec<&'a PathBuf> {
    //     self.iter_bundles()
    //         .map(|(_, records)| records.iter().map(|(name, _)| name).collect::<Vec<_>>())
    //         .flatten()
    //         .collect()
    // }

    pub fn extract<I, T>(
        &'a self,
        iter: I,
        path: impl AsRef<Path>,
        out: impl AsRef<Path>,
        shaders: bool,
    ) -> usize
    where
        I: ParallelIterator<Item = (BundleRecord, T)> + Clone,
        T: AsRef<[(PathBuf, &'a FileRecord)]>,
    {
        let bundles_path = path.as_ref().join("Bundles2");

        assert!(bundles_path.exists());
        assert!(bundles_path.is_dir());
        let out = Arc::new(out.as_ref());

        iter.filter(|(bundle, _)| shaders || !bundle.path.contains("shadercache"))
            .map(|(bundlerecord, files)| {
                let mut bundlebin_path = bundlerecord.path.to_string();
                bundlebin_path.push_str(".bundle.bin");

                let bundle_path = bundles_path.join(bundlebin_path);
                if !bundle_path.exists() {
                    #[cfg(feature = "tracing")]
                    warn!("{} doesn't exist.", bundle_path.display());

                    return 0;
                }

                let files = files.as_ref();

                let file = std::fs::read(&bundle_path).unwrap();
                let bundle: Bundle<Vec<u8>> = Bundle::from_slice(&file).unwrap();

                #[cfg(feature = "tracing")]
                info!(
                    bundle = bundlerecord.path.as_ref(),
                    "Decompressing {}.bundle.bin", bundlerecord.path,
                );

                let data = bundle.decompress().unwrap();

                let out_dir = out.clone();

                let bytes: Vec<_> = files
                    .par_iter()
                    .filter(|(path, _)| {
                        let is_shader = path
                            .components()
                            .filter_map(|c| c.as_os_str().to_str())
                            .any(|c| c.contains("shadercache"));

                        shaders || !is_shader
                    })
                    .map(|(path, info)| -> usize {
                        let start = info.offset as usize;
                        let end = start + info.size as usize;
                        let mut slice = &data[start..end];

                        let file_path = out_dir.clone().join(path);
                        let parent = file_path.parent().unwrap();

                        if !parent.exists() {
                            std::fs::create_dir_all(parent).unwrap();
                        }

                        let mut file = std::fs::File::create(&file_path).unwrap();
                        let bytes = std::io::copy(&mut slice, &mut file).unwrap();
                        assert_eq!(bytes, info.size as u64);
                        bytes as usize
                    })
                    .collect();

                #[cfg(feature = "tracing")]
                trace!(
                    done = bytes.len() as u64,
                    "Done {}.bundle.bin",
                    bundlerecord.path
                );

                bytes.iter().sum()
            })
            .sum()
    }

    // pub fn bundle_info_by_idx(
    //     &'a self,
    //     idx: usize,
    // ) -> Option<(&'a BundleRecord, Arc<&'a [(PathBuf, &'a FileRecord)]>)> {
    //     self.build_paths()
    //         .get(&idx)
    //         .map(|info| (&self.bundles[idx], info))
    // }

    pub fn iter_bundles(
        &'a self,
    ) -> impl ParallelIterator<Item = (BundleRecord, &'a Arc<[(PathBuf, &'a FileRecord)]>)> + Clone
    {
        let paths = self.build_paths();
        let bundles = &self.bundles;

        paths
            .into_par_iter()
            .map(|(&idx, info)| (bundles[idx].clone(), info))
    }

    fn build_paths(&'a self) -> &'a HashMap<usize, Arc<[(PathBuf, &'a FileRecord)]>> {
        //TODO check back later if added mutable support, cache might bite us

        self.cache.get_or_init(|| {
            let map: HashMap<_, _> = self.files.iter().map(|file| (file.hash, file)).collect();
            let bytes = &self.path_bundle.decompress().unwrap();

            let mut paths: HashMap<usize, Vec<(PathBuf, &FileRecord)>> = HashMap::new();

            for path in self.paths.iter() {
                let slice = &bytes[path.offset as usize..(path.offset + path.size) as usize];
                let mut offset = 0;
                let mut path_slice: Vec<String> = vec![];
                let mut building = read_u32(slice, &mut offset) == 0;

                while offset < path.size as usize - 4 {
                    let mut index = read_u32(slice, &mut offset);
                    if index == 0 {
                        building = !building;
                        if building {
                            path_slice.clear();
                        }
                    } else {
                        index -= 1;
                        let mut string = find_cstring(slice, &mut offset).unwrap();
                        if (index as usize) < path_slice.len() {
                            let mut prev = path_slice[index as usize].clone();
                            prev.push_str(string.as_str());
                            string = prev;
                        }
                        path_slice.push(string.clone());
                        if !building {
                            let hash = murmurhash64::murmur_hash64a(string.as_bytes(), 0x1337b33f);
                            if let Some(fr) = map.get(&hash) {
                                paths
                                    .entry(fr.bundle_idx as usize)
                                    .or_default()
                                    .push((string.into(), fr));
                            } else {
                                #[cfg(feature = "tracing")]
                                error!("Hash not found: {}", string);
                            };
                        }
                    }
                }
            }
            paths.into_iter().map(|(k, v)| (k, Arc::from(v))).collect()
        })
    }

    pub fn files(&self) {
        for file in self.files.as_ref() {
            let hash = file.hash;
            println!(
                "File: {} in Directory: {}",
                hash, self.bundles[file.bundle_idx as usize].path
            );
        }
    }
}

impl From<Index<'_>> for Vec<u8> {
    fn from(val: Index) -> Self {
        let mut data = Vec::new();

        let bundle_count = val.bundles.len() as u32;
        data.extend_from_slice(&bundle_count.to_le_bytes());
        for bundle in val.bundles.clone().as_ref() {
            data.extend_from_slice(Into::<Vec<u8>>::into(bundle).as_slice());
        }

        let file_count = val.files.len() as u32;
        data.extend_from_slice(&file_count.to_le_bytes());
        for &file in val.files.as_ref() {
            data.extend_from_slice(Into::<Vec<u8>>::into(file).as_slice());
        }

        let paths_count = val.paths.len() as u32;
        data.extend_from_slice(&paths_count.to_le_bytes());
        for &path in val.paths.as_ref() {
            data.extend_from_slice(Into::<Vec<u8>>::into(path).as_slice());
        }

        let path_bundle: Vec<u8> = val.path_bundle.into();
        data.extend(path_bundle);

        data
    }
}
impl From<Index<'_>> for Arc<[u8]> {
    fn from(val: Index) -> Self {
        let mut data = Vec::new();

        let bundle_count = val.bundles.len() as u32;
        data.extend_from_slice(&bundle_count.to_le_bytes());
        for bundle in val.bundles.clone().as_ref() {
            data.extend_from_slice(Into::<Vec<u8>>::into(bundle).as_slice());
        }

        let file_count = val.files.len() as u32;
        data.extend_from_slice(&file_count.to_le_bytes());
        for &file in val.files.as_ref() {
            data.extend_from_slice(Into::<Vec<u8>>::into(file).as_slice());
        }

        let paths_count = val.paths.len() as u32;
        data.extend_from_slice(&paths_count.to_le_bytes());
        for &path in val.paths.as_ref() {
            data.extend_from_slice(Into::<Vec<u8>>::into(path).as_slice());
        }

        let path_bundle: Vec<u8> = val.path_bundle.into();
        data.extend(path_bundle);

        data.into()
    }
}

impl From<&Index<'_>> for Vec<u8> {
    fn from(val: &Index) -> Vec<u8> {
        let mut data = Vec::new();

        let bundle_count = val.bundles.len() as u32;
        data.extend_from_slice(&bundle_count.to_le_bytes());
        for bundle in val.bundles.as_ref() {
            data.extend_from_slice(Into::<Vec<u8>>::into(bundle).as_slice());
        }

        let file_count = val.files.len() as u32;
        data.extend_from_slice(&file_count.to_le_bytes());
        for &file in val.files.as_ref() {
            data.extend_from_slice(Into::<Vec<u8>>::into(file).as_slice());
        }

        let paths_count = val.paths.len() as u32;
        data.extend_from_slice(&paths_count.to_le_bytes());
        for &path in val.paths.as_ref() {
            data.extend_from_slice(Into::<Vec<u8>>::into(path).as_slice());
        }

        let path_bundle: Vec<u8> = val.path_bundle.clone().into();
        data.extend(path_bundle);

        data
    }
}
impl From<&Index<'_>> for Arc<[u8]> {
    fn from(val: &Index) -> Self {
        let mut data = Vec::new();

        let bundle_count = val.bundles.len() as u32;
        data.extend_from_slice(&bundle_count.to_le_bytes());
        for bundle in val.bundles.as_ref() {
            data.extend_from_slice(Into::<Vec<u8>>::into(bundle).as_slice());
        }

        let file_count = val.files.len() as u32;
        data.extend_from_slice(&file_count.to_le_bytes());
        for &file in val.files.as_ref() {
            data.extend_from_slice(Into::<Vec<u8>>::into(file).as_slice());
        }

        let paths_count = val.paths.len() as u32;
        data.extend_from_slice(&paths_count.to_le_bytes());
        for &path in val.paths.as_ref() {
            data.extend_from_slice(Into::<Vec<u8>>::into(path).as_slice());
        }

        let path_bundle: Vec<u8> = val.path_bundle.clone().into();
        data.extend(path_bundle);

        data.into()
    }
}

impl TryFrom<&[u8]> for Index<'_> {
    type Error = std::io::Error;

    fn try_from(value: &[u8]) -> Result<Self, Self::Error> {
        let bundle_count = u32::from_le_bytes(value[0..4].try_into().unwrap());
        let mut bundles = Vec::with_capacity(bundle_count as usize);

        let mut offset = 4;

        for _ in 0..bundle_count {
            let record = BundleRecord::try_from(&value[offset..])?;
            offset += record.size(); // path_length, path, size

            bundles.push(record);
        }

        let file_count = u32::from_le_bytes(value[offset..offset + 4].try_into().unwrap());
        offset += 4;
        let mut files = Vec::with_capacity(file_count as usize);

        for _ in 0..file_count {
            let file_record_size = std::mem::size_of::<FileRecord>();
            let record = FileRecord::try_from(&value[offset..offset + file_record_size])?;
            offset += file_record_size;

            files.push(record);
        }

        let path_count = u32::from_le_bytes(value[offset..offset + 4].try_into().unwrap());
        offset += 4;
        let mut paths = Vec::with_capacity(path_count as usize);

        for _ in 0..path_count {
            let path_size = std::mem::size_of::<PathRecord>();
            let record = PathRecord::try_from(&value[offset..offset + path_size])?;
            offset += path_size;

            paths.push(record);
        }

        let data = value[offset..].to_vec();
        let _path_bundle = Bundle::try_from(data.as_slice()).unwrap();

        Ok(Self {
            bundles: bundles.into(),
            files: files.into(),
            paths: paths.into(),
            path_bundle: _path_bundle,
            cache: OnceLock::new(),
        })
    }
}

#[derive(Debug, Clone)]
pub struct BundleRecord {
    path: Arc<str>,
    uncompressed_size: u32,
}

impl BundleRecord {
    pub fn size(&self) -> usize {
        4 + self.path.len() + 4
    }
    pub fn path(&self) -> &str {
        &self.path
    }
}

impl From<BundleRecord> for Vec<u8> {
    fn from(val: BundleRecord) -> Self {
        let mut data = Vec::with_capacity(val.size());
        let path_len = val.path.len() as u32;

        data.extend_from_slice(&path_len.to_le_bytes());
        data.extend(val.path.as_bytes());
        data.extend_from_slice(&val.uncompressed_size.to_le_bytes());

        data
    }
}
impl From<&BundleRecord> for Vec<u8> {
    fn from(val: &BundleRecord) -> Self {
        let mut data = Vec::with_capacity(val.size());
        let path_len = val.path.len() as u32;

        data.extend_from_slice(&path_len.to_le_bytes());
        data.extend(val.path.as_bytes());
        data.extend_from_slice(&val.uncompressed_size.to_le_bytes());

        data
    }
}
impl From<BundleRecord> for Arc<[u8]> {
    fn from(val: BundleRecord) -> Self {
        let mut data = Vec::with_capacity(val.size());
        let path_len = val.path.len() as u32;

        data.extend_from_slice(&path_len.to_le_bytes());
        data.extend(val.path.as_bytes());
        data.extend_from_slice(&val.uncompressed_size.to_le_bytes());

        data.into()
    }
}

impl TryFrom<&[u8]> for BundleRecord {
    type Error = io::Error;

    fn try_from(value: &[u8]) -> Result<Self, Self::Error> {
        if value.len() < 4 {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "Insufficient data",
            ));
        }

        let str_len =
            u32::from_le_bytes(value[0..4].try_into().map_err(io::Error::other)?) as usize;

        let record_size = 4 + str_len + 4;

        if value.len() < record_size {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "Insufficient data length",
            ));
        }

        let path = std::str::from_utf8(&value[4..str_len + 4])
            .expect("Invalid UTF-8")
            .to_string()
            .into();

        let uncompressed_size =
            u32::from_le_bytes(value[str_len + 4..record_size].try_into().unwrap());

        Ok(BundleRecord {
            path,
            uncompressed_size,
        })
    }
}

#[derive(Debug, Clone, Copy)]
#[repr(C, packed)]
pub struct FileRecord {
    /// File Name in a murmurhash64
    hash: u64,
    bundle_idx: u32,
    offset: u32,
    size: u32,
}

unsafe impl Zeroable for FileRecord {}
unsafe impl Pod for FileRecord {}

impl AsRef<[u8]> for FileRecord {
    fn as_ref(&self) -> &[u8] {
        bytemuck::bytes_of(self)
    }
}

impl From<FileRecord> for Vec<u8> {
    fn from(val: FileRecord) -> Self {
        bytemuck::bytes_of(&val).to_vec()
    }
}
impl From<FileRecord> for Arc<[u8]> {
    fn from(val: FileRecord) -> Self {
        bytemuck::bytes_of(&val).to_vec().into()
    }
}

impl FileRecord {
    const FILE_RECORD_SIZE: usize = std::mem::size_of::<FileRecord>();
}

impl TryFrom<&[u8]> for FileRecord {
    type Error = std::io::Error;

    fn try_from(value: &[u8]) -> Result<Self, Self::Error> {
        if value.len() != Self::FILE_RECORD_SIZE {
            return Err(std::io::Error::new(
                std::io::ErrorKind::UnexpectedEof,
                "Insufficient data",
            ));
        }

        let hash = u64::from_le_bytes(value[0..8].try_into().unwrap());
        let bundle_index = u32::from_le_bytes(value[8..12].try_into().unwrap());
        let offset = u32::from_le_bytes(value[12..16].try_into().unwrap());
        let size = u32::from_le_bytes(value[16..20].try_into().unwrap());

        Ok(Self {
            hash,
            bundle_idx: bundle_index,
            offset,
            size,
        })
    }
}

#[derive(Debug, Clone, Copy)]
#[repr(C, packed)]
struct PathRecord {
    hash: u64,
    offset: u32,
    size: u32,
    recursive_length: u32,
}

unsafe impl Zeroable for PathRecord {}
unsafe impl Pod for PathRecord {}

impl AsRef<[u8]> for PathRecord {
    fn as_ref(&self) -> &[u8] {
        bytemuck::bytes_of(self)
    }
}

impl From<PathRecord> for Vec<u8> {
    fn from(val: PathRecord) -> Self {
        bytemuck::bytes_of(&val).to_vec()
    }
}
impl From<PathRecord> for Arc<[u8]> {
    fn from(val: PathRecord) -> Self {
        bytemuck::bytes_of(&val).to_vec().into()
    }
}

impl TryFrom<&[u8]> for PathRecord {
    type Error = std::io::Error;

    fn try_from(value: &[u8]) -> Result<Self, Self::Error> {
        const PATH_SIZE: usize = std::mem::size_of::<PathRecord>();
        if value.len() < PATH_SIZE {
            return Err(std::io::Error::new(
                std::io::ErrorKind::UnexpectedEof,
                "Insufficient data",
            ));
        }

        let hash = u64::from_le_bytes(value[0..8].try_into().unwrap());
        let offset = u32::from_le_bytes(value[8..12].try_into().unwrap());
        let size = u32::from_le_bytes(value[12..16].try_into().unwrap());
        let recursive_length = u32::from_le_bytes(value[16..20].try_into().unwrap());

        Ok(Self {
            hash,
            offset,
            size,
            recursive_length,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::Index;
    use crate::Bundle;

    #[test]
    fn decompress() {
        let slice = include_bytes!("../resources/_.index.bin");
        let bundle: Bundle<Index> = Bundle::try_from(slice.as_slice()).unwrap();

        let _ = bundle.decompress().unwrap();
    }

    #[test]
    fn compress() {
        let slice = include_bytes!("../resources/_.index.bin");
        let bundle: Bundle<Index> = Bundle::try_from(slice.as_slice()).unwrap();

        let bundle = Bundle::new(bundle.decompress().unwrap()).unwrap().to_vec();
        assert_eq!(bundle.len(), slice.len());
        assert_eq!(bundle, slice);
    }
}
