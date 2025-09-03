use crate::{config::YClassConfig, dump::ConcatenatedDumpReader};
use libloading::Library;
use memflex::external::{MemoryRegion, OwnedProcess};
use std::fs;

pub struct ManagedExtension {
    #[allow(dead_code)]
    lib: Library,
    // process id
    pid: u32,

    attach: fn(u32) -> u32,
    read: fn(usize, *mut u8, usize) -> u32,
    write: fn(usize, *const u8, usize) -> u32,
    can_read: fn(usize) -> bool,
    detach: fn(),
}

impl Drop for ManagedExtension {
    fn drop(&mut self) {
        (self.detach)();
    }
}

pub enum Process {
    Internal((OwnedProcess, Vec<MemoryRegion>)),
    Managed(ManagedExtension),
    Minidump { segments: Vec<(u64, Vec<u8>)> },
    ConcatenatedDump { reader: ConcatenatedDumpReader },
}

impl Process {
    pub fn minidump(path: impl AsRef<std::path::Path>) -> eyre::Result<Self> {
        let dump = minidump::Minidump::read_path(path)?;

        let mem = dump.get_memory().unwrap();

        let mut segments = vec![];
        let mut chunk: Option<(&[u8], u64)> = None;

        fn merge_adjacent_slices<'a, T>(a: &'a [T], b: &'a [T]) -> &'a [T] {
            assert_eq!(
                unsafe { a.as_ptr().add(a.len()) },
                b.as_ptr(),
                "Slices are not adjacent in memory"
            );
            unsafe { std::slice::from_raw_parts(a.as_ptr(), a.len() + b.len()) }
        }

        for mem in mem.by_addr() {
            let bytes = mem.bytes();
            if let Some((slice, address)) = chunk {
                // check if continuous with existing slice
                if address + slice.len() as u64 == mem.base_address() {
                    // extend existing slice
                    chunk = Some((merge_adjacent_slices(slice, bytes), address));
                } else {
                    segments.push((address, slice.to_vec()));
                    chunk = Some((bytes, mem.base_address()));
                }
            } else {
                chunk = Some((bytes, mem.base_address()));
            }
        }

        Ok(Self::Minidump { segments })
    }

    pub fn concatenated_dump(path: impl AsRef<std::path::Path>) -> eyre::Result<Self> {
        let reader = ConcatenatedDumpReader::open(path)?;
        Ok(Self::ConcatenatedDump { reader })
    }
    pub fn attach(pid: u32, config: &YClassConfig) -> eyre::Result<Self> {
        let (path, modified) = (
            config
                .plugin_path
                .clone()
                .unwrap_or_else(|| "plugin.ycpl".into()),
            config.plugin_path.is_some(),
        );

        let metadata = fs::metadata(&path);
        Ok(if metadata.is_ok() {
            let lib = unsafe { Library::new(&path)? };
            let attach = unsafe { *lib.get::<fn(u32) -> u32>(b"yc_attach")? };
            let read = unsafe { *lib.get::<fn(usize, *mut u8, usize) -> u32>(b"yc_read")? };
            let write = unsafe { *lib.get::<fn(usize, *const u8, usize) -> u32>(b"yc_write")? };
            let can_read = unsafe { *lib.get::<fn(usize) -> bool>(b"yc_can_read")? };
            let detach = unsafe { *lib.get::<fn()>(b"yc_detach")? };

            let ext = ManagedExtension {
                pid,
                lib,
                attach,
                read,
                write,
                can_read,
                detach,
            };

            (ext.attach)(pid);

            Self::Managed(ext)
        } else if modified {
            #[allow(clippy::unnecessary_unwrap)]
            return Err(metadata.unwrap_err().into());
        } else {
            #[cfg(unix)]
            let proc = memflex::external::find_process_by_id(pid)?;
            #[cfg(windows)]
            let proc = {
                use memflex::types::win::{
                    PROCESS_QUERY_INFORMATION, PROCESS_VM_READ, PROCESS_VM_WRITE,
                };

                memflex::external::open_process_by_id(
                    pid,
                    false,
                    PROCESS_VM_READ | PROCESS_VM_WRITE | PROCESS_QUERY_INFORMATION,
                )?
            };

            let maps = proc.maps()?;
            Self::Internal((proc, maps))
        })
    }

    pub fn read(&self, address: usize, buf: &mut [u8]) {
        match self {
            // TODO(ItsEthra): Proper error handling maybe?.
            Self::Internal((op, _)) => _ = op.read_buf(address, buf),
            Self::Managed(ext) => _ = (ext.read)(address, buf.as_mut_ptr(), buf.len()),
            Self::Minidump { segments } => {
                let address = address as u64;
                for (addr, mem) in segments {
                    if (*addr..*addr + mem.len() as u64).contains(&address) {
                        let base = (address - addr) as usize;
                        buf.copy_from_slice(&mem[base..base + buf.len()]);
                        break;
                    }
                }
            }
            Self::ConcatenatedDump { reader } => {
                if let Some(data) = reader.get_memory_slice(address as u64, buf.len()) {
                    let copy_len = buf.len().min(data.len());
                    buf[..copy_len].copy_from_slice(&data[..copy_len]);
                }
            }
        };
    }

    pub fn write(&self, address: usize, buf: &[u8]) {
        match self {
            // TODO(ItsEthra): Proper error handling maybe?.
            Self::Internal((op, _)) => _ = op.write_buf(address, buf),
            Self::Managed(ext) => _ = (ext.write)(address, buf.as_ptr(), buf.len()),
            Self::Minidump { .. } => { /* read only */ }
            Self::ConcatenatedDump { .. } => { /* read only */ }
        };
    }

    pub fn id(&self) -> u32 {
        match self {
            Self::Internal((op, _)) => op.id(),
            Self::Managed(ext) => ext.pid,
            Self::Minidump { .. } => 0,
            Self::ConcatenatedDump { .. } => 0,
        }
    }

    pub fn can_read(&self, address: usize) -> bool {
        match self {
            Self::Internal((_, maps)) => maps
                .iter()
                .any(|map| map.from <= address && map.to >= address && map.prot.read()),
            Self::Managed(ext) => (ext.can_read)(address),
            Self::Minidump { segments } => {
                let address = address as u64;
                for (addr, mem) in segments {
                    if (*addr..*addr + mem.len() as u64).contains(&address) {
                        return true;
                    }
                }
                false
            }
            Self::ConcatenatedDump { reader } => {
                reader.get_memory_slice(address as u64, 1).is_some()
            }
        }
    }

    pub fn name(&self) -> eyre::Result<String> {
        match self {
            Self::Internal((op, _)) => op.name().map_err(Into::into),
            Self::Managed(_) => Ok("[MANAGED]".into()),
            Self::Minidump { .. } => Ok("[minidump]".into()),
            Self::ConcatenatedDump { .. } => Ok("[concatenated dump]".into()),
        }
    }
}
