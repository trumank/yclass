//! # Concatenated Memory Dump File Format
//!
//! This module implements reading and writing of concatenated memory dump files with the following binary format:
//!
//! ## File Structure
//! ```
//! [Chunk Data 0]
//! [Chunk Data 1]
//! [Chunk Data N]
//! [Index]
//! [Index Offset]
//! ```
//!
//! ## Detailed Layout
//!
//! ### Chunk Data Section
//! - Contains the raw memory data from each chunk, concatenated sequentially
//! - No padding or alignment between chunks
//! - Order matches the sorted address order from the original dump files
//!
//! ### Index Section
//! ```
//! Offset  | Size | Type | Description
//! --------|------|------|------------
//! 0       | 8    | u64  | Number of chunks (little-endian)
//! 8       | 24   | -    | First chunk entry
//! 32      | 24   | -    | Second chunk entry
//! ...     | 24   | -    | Additional chunk entries
//! ```
//!
//! ### Chunk Entry Format (24 bytes each)
//! ```
//! Offset  | Size | Type | Description
//! --------|------|------|------------
//! 0       | 8    | u64  | Memory address (little-endian)
//! 8       | 8    | u64  | File offset to chunk data (little-endian)
//! 16      | 8    | u64  | Chunk length in bytes (little-endian)
//! ```
//!
//! ### Index Offset (Last 8 bytes of file)
//! ```
//! Offset       | Size | Type | Description
//! -------------|------|------|------------
//! file_size-8  | 8    | u64  | File offset to start of index (little-endian)
//! ```
//!
//! ## Reading Algorithm
//! 1. Seek to last 8 bytes of file
//! 2. Read index offset as u64 little-endian
//! 3. Seek to index offset
//! 4. Read number of chunks as u64 little-endian
//! 5. Read chunk entries (24 bytes each)
//! 6. Use chunk entries to locate and read specific memory regions

use std::collections::BTreeMap;
use std::fs::File;
use std::io::{self, Write};
use std::path::Path;
use memmap2::Mmap;

/// Represents a memory chunk with its address and data
#[derive(Debug, Clone)]
pub struct MemoryChunk {
    /// The virtual memory address where this chunk was located
    pub address: u64,
    /// The raw memory data
    pub data: Vec<u8>,
}

/// Represents a chunk entry in the index
#[derive(Debug, Clone, Copy)]
struct ChunkEntry {
    /// Memory address
    address: u64,
    /// File offset to chunk data
    file_offset: u64,
    /// Length of chunk data
    length: u64,
}

impl ChunkEntry {
    const SIZE: usize = 24; // 3 * 8 bytes

    fn from_bytes(bytes: &[u8]) -> io::Result<Self> {
        if bytes.len() < Self::SIZE {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "Not enough bytes for chunk entry",
            ));
        }

        let address = u64::from_le_bytes([
            bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7],
        ]);
        let file_offset = u64::from_le_bytes([
            bytes[8], bytes[9], bytes[10], bytes[11], bytes[12], bytes[13], bytes[14], bytes[15],
        ]);
        let length = u64::from_le_bytes([
            bytes[16], bytes[17], bytes[18], bytes[19], bytes[20], bytes[21], bytes[22], bytes[23],
        ]);

        Ok(Self {
            address,
            file_offset,
            length,
        })
    }

    fn to_bytes(&self) -> [u8; Self::SIZE] {
        let mut bytes = [0u8; Self::SIZE];
        bytes[0..8].copy_from_slice(&self.address.to_le_bytes());
        bytes[8..16].copy_from_slice(&self.file_offset.to_le_bytes());
        bytes[16..24].copy_from_slice(&self.length.to_le_bytes());
        bytes
    }
}

/// Concatenated memory dump reader
pub struct ConcatenatedDumpReader {
    _file: File,
    mmap: Mmap,
    chunks: BTreeMap<u64, ChunkEntry>,
}

impl ConcatenatedDumpReader {
    /// Open and parse a concatenated memory dump file
    pub fn open<P: AsRef<Path>>(path: P) -> io::Result<Self> {
        let file = File::open(&path)?;
        
        // Memory map the file
        let mmap = unsafe { Mmap::map(&file)? };
        
        if mmap.len() < 8 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "File too small to be a valid concatenated dump",
            ));
        }

        // Read index offset from last 8 bytes
        let index_offset_bytes = &mmap[mmap.len() - 8..];
        let index_offset = u64::from_le_bytes([
            index_offset_bytes[0], index_offset_bytes[1], index_offset_bytes[2], index_offset_bytes[3],
            index_offset_bytes[4], index_offset_bytes[5], index_offset_bytes[6], index_offset_bytes[7],
        ]);

        if index_offset as usize >= mmap.len() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Invalid index offset",
            ));
        }

        // Read number of chunks from index
        let chunk_count_bytes = &mmap[index_offset as usize..index_offset as usize + 8];
        let chunk_count = u64::from_le_bytes([
            chunk_count_bytes[0], chunk_count_bytes[1], chunk_count_bytes[2], chunk_count_bytes[3],
            chunk_count_bytes[4], chunk_count_bytes[5], chunk_count_bytes[6], chunk_count_bytes[7],
        ]);

        if chunk_count > (mmap.len() / ChunkEntry::SIZE) as u64 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Invalid chunk count",
            ));
        }

        // Read chunk entries
        let mut chunks = BTreeMap::new();
        let entries_start = index_offset as usize + 8;
        
        for i in 0..chunk_count {
            let entry_offset = entries_start + (i as usize * ChunkEntry::SIZE);
            if entry_offset + ChunkEntry::SIZE > mmap.len() {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "Chunk entry extends beyond file",
                ));
            }
            
            let entry_bytes = &mmap[entry_offset..entry_offset + ChunkEntry::SIZE];
            let entry = ChunkEntry::from_bytes(entry_bytes)?;
            
            // Validate chunk entry
            if entry.file_offset >= index_offset || 
               entry.file_offset + entry.length > index_offset {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "Invalid chunk file offset or length",
                ));
            }
            
            chunks.insert(entry.address, entry);
        }

        Ok(Self { _file: file, mmap, chunks })
    }

    /// Get all memory chunks in the dump
    pub fn chunks(&self) -> impl Iterator<Item = (u64, u64)> + '_ {
        self.chunks.iter().map(|(&addr, entry)| (addr, entry.length))
    }

    /// Get memory chunk slice at the given address (zero-copy)
    pub fn get_chunk_slice(&self, address: u64) -> Option<&[u8]> {
        if let Some(entry) = self.chunks.get(&address) {
            let start = entry.file_offset as usize;
            let end = start + entry.length as usize;
            Some(&self.mmap[start..end])
        } else {
            None
        }
    }

    /// Get memory slice at a specific address with length (zero-copy)
    pub fn get_memory_slice(&self, address: u64, length: usize) -> Option<&[u8]> {
        // Find the chunk that contains this address
        let chunk_entry = self.chunks
            .range(..=address)
            .next_back()
            .and_then(|(&chunk_addr, entry)| {
                if address >= chunk_addr && address < chunk_addr + entry.length {
                    Some((*entry, chunk_addr))
                } else {
                    None
                }
            });

        if let Some((entry, chunk_addr)) = chunk_entry {
            let offset_in_chunk = (address - chunk_addr) as usize;
            let available_length = (entry.length as usize).saturating_sub(offset_in_chunk);
            let read_length = length.min(available_length);

            if read_length == 0 {
                return Some(&[]);
            }

            let start = entry.file_offset as usize + offset_in_chunk;
            let end = start + read_length;
            Some(&self.mmap[start..end])
        } else {
            None
        }
    }
}

/// Concatenated memory dump writer
pub struct ConcatenatedDumpWriter {
    file: File,
    chunks: Vec<MemoryChunk>,
}

impl ConcatenatedDumpWriter {
    /// Create a new concatenated dump writer
    pub fn create<P: AsRef<Path>>(path: P) -> io::Result<Self> {
        let file = File::create(path)?;
        Ok(Self {
            file,
            chunks: Vec::new(),
        })
    }

    /// Add a memory chunk to the dump
    pub fn add_chunk(&mut self, chunk: MemoryChunk) {
        self.chunks.push(chunk);
    }

    /// Write the concatenated dump file
    pub fn write(mut self) -> io::Result<()> {
        // Sort chunks by address
        self.chunks.sort_by_key(|chunk| chunk.address);

        let mut chunk_entries = Vec::new();
        let mut current_offset = 0u64;

        // Write chunk data and build index
        for chunk in &self.chunks {
            let file_offset = current_offset;
            self.file.write_all(&chunk.data)?;
            
            chunk_entries.push(ChunkEntry {
                address: chunk.address,
                file_offset,
                length: chunk.data.len() as u64,
            });
            
            current_offset += chunk.data.len() as u64;
        }

        // Write index
        let index_offset = current_offset;
        
        // Write number of chunks
        self.file.write_all(&(chunk_entries.len() as u64).to_le_bytes())?;
        
        // Write chunk entries
        for entry in chunk_entries {
            self.file.write_all(&entry.to_bytes())?;
        }
        
        // Write index offset
        self.file.write_all(&index_offset.to_le_bytes())?;
        
        self.file.flush()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn test_chunk_entry_serialization() {
        let entry = ChunkEntry {
            address: 0x1000,
            file_offset: 0x2000,
            length: 0x100,
        };
        
        let bytes = entry.to_bytes();
        let parsed = ChunkEntry::from_bytes(&bytes).unwrap();
        
        assert_eq!(entry.address, parsed.address);
        assert_eq!(entry.file_offset, parsed.file_offset);
        assert_eq!(entry.length, parsed.length);
    }

    #[test]
    fn test_roundtrip() -> io::Result<()> {
        let temp_path = "/tmp/test_dump.dat";
        
        // Create test chunks
        let chunks = vec![
            MemoryChunk {
                address: 0x1000,
                data: b"Hello".to_vec(),
            },
            MemoryChunk {
                address: 0x2000,
                data: b"World".to_vec(),
            },
        ];
        
        // Write dump
        {
            let mut writer = ConcatenatedDumpWriter::create(temp_path)?;
            for chunk in chunks.clone() {
                writer.add_chunk(chunk);
            }
            writer.write()?;
        }
        
        // Read dump
        {
            let mut reader = ConcatenatedDumpReader::open(temp_path)?;
            
            // Check chunks exist
            let chunk_list: Vec<_> = reader.chunks().collect();
            assert_eq!(chunk_list.len(), 2);
            assert!(chunk_list.contains(&(0x1000, 5)));
            assert!(chunk_list.contains(&(0x2000, 5)));
            
            // Read chunks
            let chunk1 = reader.read_chunk(0x1000)?.unwrap();
            assert_eq!(chunk1.address, 0x1000);
            assert_eq!(chunk1.data, b"Hello");
            
            let chunk2 = reader.read_chunk(0x2000)?.unwrap();
            assert_eq!(chunk2.address, 0x2000);
            assert_eq!(chunk2.data, b"World");
            
            // Test memory reading
            let memory = reader.read_memory(0x1002, 3)?.unwrap();
            assert_eq!(memory, b"llo");
        }
        
        // Cleanup
        std::fs::remove_file(temp_path).ok();
        
        Ok(())
    }
}