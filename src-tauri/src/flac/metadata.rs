//! FLAC metadata access via libFLAC's level-2 (chain) interface.
//!
//! Thin RAII wrappers keep the unsafe surface small and make every error path
//! release the chain/iterator automatically.

use crate::flac::ffi::path_cstring;
use libflac_sys::*;
use std::path::Path;

// ─── RAII wrappers ───────────────────────────────────────────────────────────

/// Owned `FLAC__Metadata_Chain` read from a file.
pub struct MetadataChain {
    ptr: *mut FLAC__Metadata_Chain,
}

impl MetadataChain {
    /// Read the metadata chain of `path`. Handles ID3v2-prefixed files.
    pub fn read(path: &Path) -> Result<Self, String> {
        let path_c = path_cstring(path)?;
        unsafe {
            let ptr = FLAC__metadata_chain_new();
            if ptr.is_null() {
                return Err("Failed to create metadata chain".to_string());
            }
            let chain = Self { ptr };
            if FLAC__metadata_chain_read(ptr, path_c.as_ptr()) == 0 {
                return Err(format!(
                    "Failed to read metadata chain: {}",
                    chain.status_string()
                ));
            }
            Ok(chain)
        }
    }

    /// Write the chain back to the file it was read from.
    /// `use_padding`: let libFLAC absorb size changes into existing PADDING
    /// instead of rewriting the whole file.
    pub fn write(&self, use_padding: bool) -> Result<(), String> {
        unsafe {
            if FLAC__metadata_chain_write(self.ptr, use_padding as FLAC__bool, 0) == 0 {
                return Err(format!(
                    "Failed to write metadata chain: {}",
                    self.status_string()
                ));
            }
        }
        Ok(())
    }

    /// Merge adjacent PADDING blocks and move padding to the end of the chain.
    pub fn sort_padding(&self) {
        unsafe { FLAC__metadata_chain_sort_padding(self.ptr) }
    }

    pub fn iter(&self) -> Result<MetadataIter<'_>, String> {
        MetadataIter::new(self)
    }

    fn status_string(&self) -> String {
        unsafe {
            let status = FLAC__metadata_chain_status(self.ptr);
            if status > FLAC__METADATA_CHAIN_STATUS_WRONG_WRITE_CALL {
                return format!("#{status}");
            }
            let entry = *FLAC__Metadata_ChainStatusString
                .as_ptr()
                .add(status as usize);
            if entry.is_null() {
                format!("#{status}")
            } else {
                std::ffi::CStr::from_ptr(entry)
                    .to_string_lossy()
                    .into_owned()
            }
        }
    }
}

impl Drop for MetadataChain {
    fn drop(&mut self) {
        unsafe { FLAC__metadata_chain_delete(self.ptr) }
    }
}

/// Cursor over the blocks of a [`MetadataChain`]. Starts at STREAMINFO.
pub struct MetadataIter<'a> {
    ptr: *mut FLAC__Metadata_Iterator,
    chain: &'a MetadataChain,
}

impl<'a> MetadataIter<'a> {
    fn new(chain: &'a MetadataChain) -> Result<Self, String> {
        unsafe {
            let ptr = FLAC__metadata_iterator_new();
            if ptr.is_null() {
                return Err("Failed to create metadata iterator".to_string());
            }
            FLAC__metadata_iterator_init(ptr, chain.ptr);
            Ok(Self { ptr, chain })
        }
    }

    /// Rewind to the first block (STREAMINFO).
    pub fn reset(&self) {
        unsafe { FLAC__metadata_iterator_init(self.ptr, self.chain.ptr) }
    }

    /// Current block (never null for a valid chain).
    pub fn block(&self) -> *mut FLAC__StreamMetadata {
        unsafe { FLAC__metadata_iterator_get_block(self.ptr) }
    }

    pub fn block_type(&self) -> FLAC__MetadataType {
        unsafe { (*self.block()).type_ }
    }

    /// Advance. Returns false when already at the last block.
    pub fn next(&self) -> bool {
        unsafe { FLAC__metadata_iterator_next(self.ptr) != 0 }
    }

    /// Move to the last block.
    pub fn seek_end(&self) {
        while self.next() {}
    }

    /// Delete the current block. The iterator is left on the *previous*
    /// block. Fails on STREAMINFO.
    pub fn delete_block(&self, replace_with_padding: bool) -> bool {
        unsafe {
            FLAC__metadata_iterator_delete_block(self.ptr, replace_with_padding as FLAC__bool) != 0
        }
    }

    /// Insert `block` after the current one and move onto it. On success the
    /// chain takes ownership of `block`.
    ///
    /// # Safety
    /// `block` must be a valid metadata object created by libFLAC that is not
    /// owned by any chain; on success it must not be freed by the caller.
    pub unsafe fn insert_block_after(&self, block: *mut FLAC__StreamMetadata) -> bool {
        FLAC__metadata_iterator_insert_block_after(self.ptr, block) != 0
    }

    /// Find the first block of `kind` starting from the current position.
    pub fn find(&self, kind: FLAC__MetadataType) -> Option<*mut FLAC__StreamMetadata> {
        loop {
            if self.block_type() == kind {
                return Some(self.block());
            }
            if !self.next() {
                return None;
            }
        }
    }
}

impl Drop for MetadataIter<'_> {
    fn drop(&mut self) {
        unsafe { FLAC__metadata_iterator_delete(self.ptr) }
    }
}

/// A set of standalone metadata block objects (not attached to a chain),
/// freed on drop. Used to hand metadata to the encoder.
pub struct MetadataBlocks {
    ptrs: Vec<*mut FLAC__StreamMetadata>,
}

impl MetadataBlocks {
    pub fn new() -> Self {
        Self { ptrs: Vec::new() }
    }

    pub fn len(&self) -> usize {
        self.ptrs.len()
    }

    pub fn is_empty(&self) -> bool {
        self.ptrs.is_empty()
    }

    /// Take ownership of `block` (appended at the end).
    pub fn push(&mut self, block: *mut FLAC__StreamMetadata) {
        self.ptrs.push(block);
    }

    /// Take ownership of `block`, placing it at `index`.
    pub fn insert(&mut self, index: usize, block: *mut FLAC__StreamMetadata) {
        self.ptrs.insert(index.min(self.ptrs.len()), block);
    }

    pub fn find(&self, kind: FLAC__MetadataType) -> Option<*mut FLAC__StreamMetadata> {
        self.ptrs
            .iter()
            .copied()
            .find(|&b| unsafe { (*b).type_ } == kind)
    }

    /// Pointer suitable for `FLAC__stream_encoder_set_metadata`.
    pub fn as_mut_ptr(&mut self) -> *mut *mut FLAC__StreamMetadata {
        self.ptrs.as_mut_ptr()
    }

    /// Give up ownership of every block (the caller must free them or hand
    /// them to a chain).
    pub fn take_all(&mut self) -> Vec<*mut FLAC__StreamMetadata> {
        std::mem::take(&mut self.ptrs)
    }
}

/// Splice cloned source blocks into a freshly encoded file.
///
/// Uses the chain API, which — unlike attaching blocks to the encoder — does
/// not validate comment names, UTF-8, picture headers or the number of
/// VORBIS_COMMENT blocks, so whatever a tagger wrote is carried over
/// byte-for-byte. The encoder's own (empty) VORBIS_COMMENT is replaced in
/// place by the source's first VORBIS_COMMENT, vendor string included; every
/// other block is appended in source order.
pub fn apply_metadata_blocks(dst: &Path, mut blocks: MetadataBlocks) -> Result<(), String> {
    let chain = MetadataChain::read(dst)?;
    let iter = chain.iter()?;
    let mut pending = std::collections::VecDeque::from(blocks.take_all());

    // Frees whatever has not been handed to the chain yet.
    let free_rest = |pending: &mut std::collections::VecDeque<*mut FLAC__StreamMetadata>| {
        for b in pending.drain(..) {
            unsafe { FLAC__metadata_object_delete(b) }
        }
    };

    let first_vc = pending
        .iter()
        .position(|&b| unsafe { (*b).type_ } == FLAC__METADATA_TYPE_VORBIS_COMMENT);
    if let Some(idx) = first_vc {
        let src_vc = pending.remove(idx).expect("index from position");
        iter.reset();
        let inserted = match iter.find(FLAC__METADATA_TYPE_VORBIS_COMMENT) {
            // delete_block parks the iterator on the previous block, so the
            // insert lands exactly where the encoder's block was.
            Some(_) => iter.delete_block(false) && unsafe { iter.insert_block_after(src_vc) },
            None => {
                iter.reset();
                unsafe { iter.insert_block_after(src_vc) }
            }
        };
        if !inserted {
            unsafe { FLAC__metadata_object_delete(src_vc) };
            free_rest(&mut pending);
            return Err("Failed to insert VORBIS_COMMENT block".to_string());
        }
    }

    iter.seek_end();
    while let Some(block) = pending.pop_front() {
        if !unsafe { iter.insert_block_after(block) } {
            unsafe { FLAC__metadata_object_delete(block) };
            free_rest(&mut pending);
            return Err("Failed to insert metadata block".to_string());
        }
    }
    drop(iter);

    chain.sort_padding();
    chain.write(false)
}

/// Raw view of every block of a given type, in file order. Used by the
/// post-encode equivalence check and by tests.
pub fn read_vorbis_comments(path: &Path) -> Result<Vec<VorbisCommentBlock>, String> {
    let chain = MetadataChain::read(path)?;
    let iter = chain.iter()?;
    let mut out = Vec::new();
    loop {
        if iter.block_type() == FLAC__METADATA_TYPE_VORBIS_COMMENT {
            out.push(unsafe { VorbisCommentBlock::from_raw(iter.block()) });
        }
        if !iter.next() {
            break;
        }
    }
    Ok(out)
}

/// Byte-exact copy of a VORBIS_COMMENT block's contents.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VorbisCommentBlock {
    pub vendor: Vec<u8>,
    pub entries: Vec<Vec<u8>>,
}

impl VorbisCommentBlock {
    /// # Safety
    /// `block` must be a valid VORBIS_COMMENT metadata object.
    pub unsafe fn from_raw(block: *mut FLAC__StreamMetadata) -> Self {
        let vc = &(*block).data.vorbis_comment;
        let entry_bytes = |e: &FLAC__StreamMetadata_VorbisComment_Entry| -> Vec<u8> {
            if e.entry.is_null() || e.length == 0 {
                Vec::new()
            } else {
                std::slice::from_raw_parts(e.entry, e.length as usize).to_vec()
            }
        };
        let vendor = entry_bytes(&vc.vendor_string);
        let entries = (0..vc.num_comments as usize)
            .map(|i| entry_bytes(&*vc.comments.add(i)))
            .collect();
        Self { vendor, entries }
    }
}

impl Default for MetadataBlocks {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for MetadataBlocks {
    fn drop(&mut self) {
        for &b in &self.ptrs {
            unsafe { FLAC__metadata_object_delete(b) }
        }
    }
}

// ─── Queries ─────────────────────────────────────────────────────────────────

/// Read the embedded MD5 (hex) from a FLAC file's STREAMINFO.
pub async fn get_md5sum(flac_path: &Path) -> Result<String, String> {
    let flac_path = flac_path.to_path_buf();
    tokio::task::spawn_blocking(move || get_md5sum_native(&flac_path))
        .await
        .map_err(|e| format!("MD5 task panicked: {e}"))?
}

fn get_md5sum_native(flac_path: &Path) -> Result<String, String> {
    let path_c = path_cstring(flac_path)?;
    unsafe {
        // Must be a full FLAC__StreamMetadata: libFLAC writes type/is_last/
        // length ahead of the stream_info union member.
        let mut metadata: FLAC__StreamMetadata = std::mem::zeroed();
        if FLAC__metadata_get_streaminfo(path_c.as_ptr(), &mut metadata) == 0 {
            return Err("Failed to read STREAMINFO".to_string());
        }
        Ok(metadata
            .data
            .stream_info
            .md5sum
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect())
    }
}

/// Block types carried over from the source file to the re-encoded output.
/// STREAMINFO is regenerated by the encoder, PADDING is intentionally dropped,
/// SEEKTABLE is not preserved (matches the behaviour of previous releases).
const COPIED_BLOCK_TYPES: [FLAC__MetadataType; 4] = [
    FLAC__METADATA_TYPE_VORBIS_COMMENT,
    FLAC__METADATA_TYPE_PICTURE,
    FLAC__METADATA_TYPE_APPLICATION,
    FLAC__METADATA_TYPE_CUESHEET,
];

/// Clone every tag/artwork/application/cuesheet block of `src` into
/// standalone objects, in file order, ready to be attached to an encoder.
pub fn clone_metadata_blocks(src: &Path) -> Result<MetadataBlocks, String> {
    let chain = MetadataChain::read(src)?;
    let iter = chain.iter()?;
    let mut blocks = MetadataBlocks::new();
    loop {
        if COPIED_BLOCK_TYPES.contains(&iter.block_type()) {
            let copy = unsafe { FLAC__metadata_object_clone(iter.block()) };
            if copy.is_null() {
                return Err("Failed to clone metadata block (out of memory)".to_string());
            }
            blocks.push(copy);
        }
        if !iter.next() {
            break;
        }
    }
    Ok(blocks)
}

// ─── Non-FLAC prefix (ID3v2) handling ────────────────────────────────────────

const FLAC_MARKER: &[u8; 4] = b"fLaC";
/// How far into a file we are willing to look for the `fLaC` marker when the
/// prefix is not a well-formed ID3v2 tag.
const PREFIX_SCAN_LIMIT: usize = 1024 * 1024;

/// Extract any bytes that precede the FLAC stream marker (`fLaC`).
///
/// Some taggers prepend an ID3v2 header to FLAC files. libFLAC skips it when
/// decoding, so the re-encoded output would silently lose it; we preserve the
/// bytes verbatim and re-attach them after encoding.
///
/// Reads only as much of the file as needed (the old implementation read the
/// entire file into memory just to inspect the first four bytes).
pub fn extract_non_flac_prefix(path: &Path) -> Result<Vec<u8>, String> {
    use std::io::Read;
    let mut file = std::fs::File::open(path).map_err(|e| format!("open: {e}"))?;

    let mut head = [0u8; 10];
    let n = file.read(&mut head).map_err(|e| format!("read: {e}"))?;
    if n >= 4 && &head[..4] == FLAC_MARKER {
        return Ok(Vec::new());
    }

    // Fast path: well-formed ID3v2 header → size is known exactly.
    if n == 10 && &head[..3] == b"ID3" {
        let flags = head[5];
        let size = head[6..10]
            .iter()
            .fold(0usize, |acc, &b| (acc << 7) | (b & 0x7F) as usize);
        let footer = if flags & 0x10 != 0 { 10 } else { 0 };
        let total = 10 + size + footer;
        if total <= PREFIX_SCAN_LIMIT {
            let mut buf = head.to_vec();
            buf.resize(total + 4, 0);
            let got = read_fully(&mut file, &mut buf[10..])?;
            if got == total + 4 - 10 && &buf[total..total + 4] == FLAC_MARKER {
                buf.truncate(total);
                return Ok(buf);
            }
            // Malformed size: fall through to the scan below using what we
            // already read.
            buf.truncate(10 + got);
            return scan_for_marker(&buf, &mut file);
        }
    }

    scan_for_marker(&head[..n], &mut file)
}

fn read_fully(file: &mut std::fs::File, buf: &mut [u8]) -> Result<usize, String> {
    use std::io::Read;
    let mut total = 0;
    while total < buf.len() {
        let n = file
            .read(&mut buf[total..])
            .map_err(|e| format!("read: {e}"))?;
        if n == 0 {
            break;
        }
        total += n;
    }
    Ok(total)
}

/// Slow path: look for `fLaC` within the first `PREFIX_SCAN_LIMIT` bytes.
fn scan_for_marker(already: &[u8], file: &mut std::fs::File) -> Result<Vec<u8>, String> {
    let mut buf = already.to_vec();
    buf.resize(PREFIX_SCAN_LIMIT, 0);
    let got = read_fully(file, &mut buf[already.len()..])?;
    buf.truncate(already.len() + got);
    match buf.windows(4).position(|w| w == FLAC_MARKER) {
        Some(0) | None => Ok(Vec::new()),
        Some(pos) => Ok(buf[..pos].to_vec()),
    }
}

/// Prepend raw bytes to an existing file (used to re-attach a preserved
/// ID3v2 prefix after re-encoding).
pub fn prepend_bytes_to_file(path: &Path, prefix: &[u8]) -> Result<(), String> {
    if prefix.is_empty() {
        return Ok(());
    }
    use std::io::Write;
    let body = std::fs::read(path).map_err(|e| format!("read for prepend: {e}"))?;
    let mut f = std::fs::File::create(path).map_err(|e| format!("create for prepend: {e}"))?;
    f.write_all(prefix)
        .map_err(|e| format!("write prefix: {e}"))?;
    f.write_all(&body).map_err(|e| format!("write body: {e}"))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> Option<std::path::PathBuf> {
        let p = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .join("Tests")
            .join("un-optimized.flac");
        p.exists().then_some(p)
    }

    #[test]
    fn test_prefix_plain_flac_is_empty() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("a.flac");
        std::fs::write(&p, b"fLaC\0\0\0\x22rest").unwrap();
        assert!(extract_non_flac_prefix(&p).unwrap().is_empty());
    }

    #[test]
    fn test_prefix_id3v2_exact_size() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("a.flac");
        // ID3v2.3 header, no footer, payload size 5 (syncsafe).
        let mut data = b"ID3\x03\x00\x00\x00\x00\x00\x05".to_vec();
        data.extend_from_slice(b"ABCDE");
        data.extend_from_slice(b"fLaC");
        data.extend_from_slice(&[0u8; 64]);
        std::fs::write(&p, &data).unwrap();
        let prefix = extract_non_flac_prefix(&p).unwrap();
        assert_eq!(prefix.len(), 15);
        assert_eq!(&prefix[..3], b"ID3");
    }

    #[test]
    fn test_prefix_id3v2_with_footer_flag() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("a.flac");
        let mut data = b"ID3\x04\x00\x10\x00\x00\x00\x02".to_vec(); // footer flag set
        data.extend_from_slice(b"XY"); // 2-byte payload
        data.extend_from_slice(b"3DI\x04\x00\x10\x00\x00\x00\x02"); // 10-byte footer
        data.extend_from_slice(b"fLaC");
        std::fs::write(&p, &data).unwrap();
        let prefix = extract_non_flac_prefix(&p).unwrap();
        assert_eq!(prefix.len(), 22);
    }

    #[test]
    fn test_prefix_malformed_id3_falls_back_to_scan() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("a.flac");
        // Claims a 100-byte payload but the marker shows up after 3 bytes.
        let mut data = b"ID3\x03\x00\x00\x00\x00\x00\x64".to_vec();
        data.extend_from_slice(b"abc");
        data.extend_from_slice(b"fLaC");
        data.extend_from_slice(&[0u8; 8]);
        std::fs::write(&p, &data).unwrap();
        let prefix = extract_non_flac_prefix(&p).unwrap();
        assert_eq!(prefix.len(), 13);
    }

    #[test]
    fn test_prefix_arbitrary_junk_then_marker() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("a.flac");
        let mut data = vec![0x42u8; 300];
        data.extend_from_slice(b"fLaC");
        std::fs::write(&p, &data).unwrap();
        assert_eq!(extract_non_flac_prefix(&p).unwrap().len(), 300);
    }

    #[test]
    fn test_prefix_no_marker_is_empty() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("a.flac");
        std::fs::write(&p, vec![1u8; 500]).unwrap();
        assert!(extract_non_flac_prefix(&p).unwrap().is_empty());
    }

    #[test]
    fn test_prepend_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("a.bin");
        std::fs::write(&p, b"body").unwrap();
        prepend_bytes_to_file(&p, b"head-").unwrap();
        assert_eq!(std::fs::read(&p).unwrap(), b"head-body");
        prepend_bytes_to_file(&p, b"").unwrap();
        assert_eq!(std::fs::read(&p).unwrap(), b"head-body");
    }

    #[test]
    fn test_chain_read_missing_file_errors() {
        assert!(MetadataChain::read(Path::new("/nope/missing.flac")).is_err());
    }

    #[test]
    fn test_clone_metadata_blocks_from_fixture() {
        let Some(src) = fixture() else {
            eprintln!("fixture missing — skipping");
            return;
        };
        let blocks = clone_metadata_blocks(&src).unwrap();
        assert!(!blocks.is_empty());
        assert!(blocks.find(FLAC__METADATA_TYPE_VORBIS_COMMENT).is_some());
        assert!(blocks.find(FLAC__METADATA_TYPE_PICTURE).is_some());
        assert!(blocks.find(FLAC__METADATA_TYPE_STREAMINFO).is_none());
        assert!(blocks.find(FLAC__METADATA_TYPE_PADDING).is_none());
    }

    #[test]
    fn test_get_md5sum_from_fixture_is_32_hex() {
        let Some(src) = fixture() else {
            return;
        };
        let rt = tokio::runtime::Runtime::new().unwrap();
        let md5 = rt.block_on(get_md5sum(&src)).unwrap();
        assert_eq!(md5.len(), 32);
        assert!(md5.chars().all(|c| c.is_ascii_hexdigit()));
    }
}
