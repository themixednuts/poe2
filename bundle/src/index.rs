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
    bundles: Vec<BundleRecord>,
    files: Vec<FileRecord>,
    paths: Vec<PathRecord>,
    path_bundle: Bundle<Vec<u8>>,
    cache: OnceLock<HashMap<usize, Vec<(PathBuf, &'a FileRecord)>>>,
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

    pub fn list_bundles(&'a self) -> Vec<&'a PathBuf> {
        self.iter_bundles()
            .map(|(_, records)| records.iter().map(|(name, _)| name).collect::<Vec<_>>())
            .flatten()
            .collect()
    }

    fn extract<I, T>(
        &'a self,
        iter: I,
        path: impl AsRef<Path>,
        out: impl AsRef<Path>,
        shaders: bool,
    ) -> usize
    where
        I: ParallelIterator<Item = (BundleRecord, T)>,
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
                    bundle = bundlerecord.path,
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
                            std::fs::create_dir_all(&parent).unwrap();
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

    pub fn extract_all(
        &'a self,
        bundles_path: impl AsRef<Path>,
        out_dir: impl AsRef<Path>,
        shaders: bool,
    ) -> usize {
        self.extract(self.iter_bundles(), &bundles_path, &out_dir, shaders)
    }

    pub fn extract_files(
        &'a self,
        pattern: impl AsRef<str>,
        bundles_path: impl AsRef<Path>,
        out_dir: impl AsRef<Path>,
        shaders: bool,
    ) -> usize {
        let mut builder = GlobSetBuilder::new();
        pattern.as_ref().split(',').for_each(|pat| {
            builder.add(Glob::new(pat).unwrap());
        });
        let pattern = builder.build().unwrap();
        let filtered = self.iter_bundles().filter_map(|(bundle, files)| {
            let matching = files
                .into_iter()
                .filter(|(path, _)| pattern.is_match(path.to_str().unwrap()))
                .cloned()
                .collect::<Vec<_>>();

            if matching.is_empty() {
                None
            } else {
                Some((bundle, matching))
            }
        });
        self.extract(filtered, &bundles_path, &out_dir, shaders)
    }

    pub fn bundle_info_by_idx(
        &'a self,
        idx: usize,
    ) -> Option<(&'a BundleRecord, &'a Vec<(PathBuf, &'a FileRecord)>)> {
        self.build_paths()
            .get(&idx)
            .map(|info| (&self.bundles[idx], info))
    }

    pub fn iter_bundles(
        &'a self,
    ) -> impl ParallelIterator<Item = (BundleRecord, &'a Vec<(PathBuf, &'a FileRecord)>)> {
        // TODO figure out parrellel later.......hmmm
        // if we figure it out, we'll need to revisit the thread work between bundle.decompress and this

        let paths = self.build_paths();
        let bundles = &self.bundles;

        paths
            .par_iter()
            .map(move |(&idx, info)| (bundles[idx].clone(), info))
    }

    fn build_paths(&'a self) -> &'a HashMap<usize, Vec<(PathBuf, &'a FileRecord)>> {
        //TODO check back later if added mutable support, cache might bite us

        self.cache.get_or_init(|| {
            let map: HashMap<_, _> = self.files.iter().map(|file| (file.hash, file)).collect();
            let bytes = &self.path_bundle.decompress().unwrap();

            let mut paths: HashMap<usize, Vec<(PathBuf, &FileRecord)>> = HashMap::new();

            for (_i, path) in self.paths.iter().enumerate() {
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
                            // TODO do we need clone?
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
                                    .push((string.into(), &fr));
                            } else {
                                #[cfg(feature = "tracing")]
                                error!("Hash not found: {}", string);
                            };
                        }
                    }
                }
            }
            paths
        })
    }

    // TODO Find the link to paths
    pub fn files(&self) {
        for file in &self.files {
            let hash = file.hash;
            println!(
                "File: {} in Directory: {}",
                hash, self.bundles[file.bundle_idx as usize].path
            );
        }
    }
}

impl<'a> Into<Vec<u8>> for Index<'a> {
    fn into(self) -> Vec<u8> {
        let mut data = Vec::new();

        let bundle_count = self.bundles.len() as u32;
        data.extend_from_slice(&bundle_count.to_le_bytes());
        for bundle in self.bundles {
            data.extend_from_slice(Into::<Vec<u8>>::into(bundle).as_slice());
        }

        let file_count = self.files.len() as u32;
        data.extend_from_slice(&file_count.to_le_bytes());
        for file in self.files {
            data.extend_from_slice(Into::<Vec<u8>>::into(file).as_slice());
        }

        let paths_count = self.paths.len() as u32;
        data.extend_from_slice(&paths_count.to_le_bytes());
        for path in self.paths {
            data.extend_from_slice(Into::<Vec<u8>>::into(path).as_slice());
        }

        let path_bundle: Vec<u8> = self.path_bundle.into();
        data.extend(path_bundle);

        data
    }
}

impl<'a> Into<Vec<u8>> for &Index<'a> {
    fn into(self) -> Vec<u8> {
        let mut data = Vec::new();

        let bundle_count = self.bundles.len() as u32;
        data.extend_from_slice(&bundle_count.to_le_bytes());
        for bundle in self.bundles.clone() {
            data.extend_from_slice(Into::<Vec<u8>>::into(bundle).as_slice());
        }

        let file_count = self.files.len() as u32;
        data.extend_from_slice(&file_count.to_le_bytes());
        for file in self.files.clone() {
            data.extend_from_slice(Into::<Vec<u8>>::into(file).as_slice());
        }

        let paths_count = self.paths.len() as u32;
        data.extend_from_slice(&paths_count.to_le_bytes());
        for path in self.paths.clone() {
            data.extend_from_slice(Into::<Vec<u8>>::into(path).as_slice());
        }

        let path_bundle: Vec<u8> = self.path_bundle.clone().into();
        data.extend(path_bundle);

        data
    }
}

impl<'a> TryFrom<&[u8]> for Index<'a> {
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
            bundles,
            files,
            paths,
            path_bundle: _path_bundle,
            cache: OnceLock::new(),
        })
    }
}

#[derive(Debug, Clone)]
pub struct BundleRecord {
    path: String,
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

impl Into<Vec<u8>> for BundleRecord {
    fn into(self) -> Vec<u8> {
        let mut data = Vec::with_capacity(self.size());
        let path_len = self.path.len() as u32;

        data.extend_from_slice(&path_len.to_le_bytes());
        data.extend(self.path.into_bytes());
        data.extend_from_slice(&self.uncompressed_size.to_le_bytes());

        data
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
            u32::from_le_bytes(value[0..4].try_into().map_err(|e| io::Error::other(e))?) as usize;

        let record_size = 4 + str_len + 4;

        if value.len() < record_size {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "Insufficient data length",
            ));
        }

        let path = std::str::from_utf8(&value[4..str_len + 4])
            .expect("Invalid UTF-8")
            .to_string();

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

impl Into<Vec<u8>> for FileRecord {
    fn into(self) -> Vec<u8> {
        bytemuck::bytes_of(&self).to_vec()
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

impl Into<Vec<u8>> for PathRecord {
    fn into(self) -> Vec<u8> {
        bytemuck::bytes_of(&self).to_vec()
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
