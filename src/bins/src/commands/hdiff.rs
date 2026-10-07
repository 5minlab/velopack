use anyhow::{bail, Context, Result};
use std::{
    ffi::c_void,
    fs::{self, File},
    io::{Read, Seek, SeekFrom, Write},
    path::Path,
};

type ReadCallback = unsafe extern "C" fn(*mut c_void, u64, *mut u8, usize) -> i32;
type WriteCallback = unsafe extern "C" fn(*mut c_void, u64, *const u8, usize) -> i32;

extern "C" {
    fn velopack_hpatch(
        old: *mut c_void,
        old_size: u64,
        diff: *mut c_void,
        diff_size: u64,
        output: *mut c_void,
        expected_size: u64,
        read: ReadCallback,
        write: WriteCallback,
        cache: *mut u8,
        cache_size: usize,
    ) -> i32;
}

struct PatchFile {
    file: File,
    error: Option<std::io::Error>,
}

impl PatchFile {
    fn new(file: File) -> Self {
        Self { file, error: None }
    }

    fn handle(&mut self) -> *mut c_void {
        self as *mut Self as *mut c_void
    }
}

unsafe extern "C" fn read(handle: *mut c_void, offset: u64, data: *mut u8, len: usize) -> i32 {
    // The C bridge passes live, exclusive handles and valid buffers for this synchronous call.
    let context = &mut *(handle as *mut PatchFile);
    let buffer = std::slice::from_raw_parts_mut(data, len);
    match context.file.seek(SeekFrom::Start(offset)).and_then(|_| context.file.read_exact(buffer)) {
        Ok(_) => 1,
        Err(error) => {
            context.error = Some(error);
            0
        }
    }
}

unsafe extern "C" fn write(handle: *mut c_void, offset: u64, data: *const u8, len: usize) -> i32 {
    let context = &mut *(handle as *mut PatchFile);
    let buffer = std::slice::from_raw_parts(data, len);
    match context.file.seek(SeekFrom::Start(offset)).and_then(|_| context.file.write_all(buffer)) {
        Ok(_) => 1,
        Err(error) => {
            context.error = Some(error);
            0
        }
    }
}

/// Apply an uncompressed HDIFF13 patch with bounded working memory and verify its output.
/// The output must not exist. Failed patches are removed; the old file is never modified.
pub fn hdiff_patch_single(old: &Path, patch: &Path, output: &Path, size: u64, sha1: &str) -> Result<()> {
    if sha1.len() != 40 || !sha1.bytes().all(|c| c.is_ascii_hexdigit()) {
        bail!("Invalid HDiffPatch output checksum");
    }
    let mut old = PatchFile::new(File::open(old)?);
    let mut patch = PatchFile::new(File::open(patch)?);
    let old_size = old.file.metadata()?.len();
    let patch_size = patch.file.metadata()?.len();
    let output_file = fs::OpenOptions::new().read(true).write(true).create_new(true).open(output)?;
    let result = (|| {
        let mut output = PatchFile::new(output_file);
        let mut cache = vec![0u8; 1024 * 1024];
        // All handles and cache remain alive throughout the call; HPatch retains no pointers.
        let success = unsafe {
            velopack_hpatch(
                old.handle(), old_size, patch.handle(), patch_size, output.handle(), size,
                read, write, cache.as_mut_ptr(), cache.len(),
            )
        };
        for error in [old.error.take(), patch.error.take(), output.error.take()].into_iter().flatten() {
            return Err(error).context("HDiffPatch file I/O failed");
        }
        if success == 0 {
            bail!("Invalid, unsupported, or corrupt HDIFF13 patch");
        }
        if output.file.metadata()?.len() != size {
            bail!("HDiffPatch output size mismatch");
        }
        output.file.rewind()?;
        let mut digest = sha1_smol::Sha1::new();
        loop {
            let count = output.file.read(&mut cache)?;
            if count == 0 {
                break;
            }
            digest.update(&cache[..count]);
        }
        if !digest.digest().to_string().eq_ignore_ascii_case(sha1) {
            bail!("HDiffPatch output checksum mismatch");
        }
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(output);
    }
    result
}

pub(super) fn read_shasum(path: &Path) -> Result<(u64, String)> {
    let contents = fs::read_to_string(path).context("Missing HDiffPatch checksum sidecar")?;
    let contents = contents.trim_start_matches('\u{feff}').trim();
    // ReleaseEntry permits spaces in filenames; only the first and last fields matter.
    let (hash, rest) = contents.split_once(' ').context("Invalid checksum sidecar")?;
    let (_, size) = rest.rsplit_once(' ').context("Invalid checksum sidecar")?;
    if hash.len() != 40 || !hash.bytes().all(|c| c.is_ascii_hexdigit()) {
        bail!("Invalid checksum sidecar");
    }
    Ok((size.parse().context("Invalid checksum size")?, hash.to_owned()))
}

#[cfg(test)]
mod tests {
    use super::*;

    const OLD: &[u8] = include_bytes!("../../../../test/fixtures/hdiffpatch/old.bin");
    const NEW: &[u8] = include_bytes!("../../../../test/fixtures/hdiffpatch/new.bin");
    const SPEED: &[u8] = include_bytes!("../../../../test/fixtures/hdiffpatch/speed.hdiff");
    const SIZE: &[u8] = include_bytes!("../../../../test/fixtures/hdiffpatch/size.hdiff");

    fn run(old: &[u8], patch: &[u8], expected: &[u8], valid: bool) {
        let temp = tempfile::tempdir().unwrap();
        let old_path = temp.path().join("old 한 글.bin");
        let patch_path = temp.path().join("patch.hdiff");
        let output = temp.path().join("new 한 글.bin");
        fs::write(&old_path, old).unwrap();
        fs::write(&patch_path, patch).unwrap();
        let hash = sha1_smol::Sha1::from(expected).digest().to_string();
        let result = hdiff_patch_single(&old_path, &patch_path, &output, expected.len() as u64, &hash);
        if valid {
            result.unwrap();
            assert_eq!(fs::read(&output).unwrap(), expected);
        } else {
            assert!(result.is_err());
            assert!(!output.exists(), "failed patch must be removed");
        }
        assert_eq!(fs::read(old_path).unwrap(), old, "old file must be preserved");
    }

    #[test]
    fn applies_both_generation_modes() {
        run(OLD, SPEED, NEW, true);
        run(OLD, SIZE, NEW, true);
    }

    #[test]
    fn handles_empty_files() {
        run(OLD, include_bytes!("../../../../test/fixtures/hdiffpatch/to-empty.hdiff"), b"", true);
        run(b"", include_bytes!("../../../../test/fixtures/hdiffpatch/from-empty.hdiff"), NEW, true);
    }

    #[test]
    fn rejects_truncated_and_invalid_patches() {
        for end in 0..SPEED.len() {
            run(OLD, &SPEED[..end], NEW, false);
        }
        run(OLD, b"not a patch", NEW, false);
    }

    #[test]
    fn rejects_wrong_base_and_checksum_and_size() {
        run(&vec![0; OLD.len()], SPEED, NEW, false);
        run(&OLD[..OLD.len() - 1], SPEED, NEW, false);
        run(OLD, SPEED, &vec![0; NEW.len()], false);
        run(OLD, SPEED, &NEW[..NEW.len() - 1], false);
    }

    #[test]
    fn does_not_overwrite_existing_output() {
        let temp = tempfile::tempdir().unwrap();
        let old = temp.path().join("old");
        let patch = temp.path().join("patch");
        fs::write(&old, OLD).unwrap();
        fs::write(&patch, SPEED).unwrap();
        let hash = sha1_smol::Sha1::from(NEW).digest().to_string();
        assert!(hdiff_patch_single(&old, &patch, &old, NEW.len() as u64, &hash).is_err());
        assert_eq!(fs::read(old).unwrap(), OLD);
    }

    #[test]
    fn parses_checksum_sidecars_with_bom_and_spaces() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("file.shasum");
        let hash = sha1_smol::Sha1::from(NEW).digest().to_string();
        fs::write(&path, format!("\u{feff}{hash} file name.shasum {}\n", NEW.len())).unwrap();
        assert_eq!(read_shasum(&path).unwrap(), (NEW.len() as u64, hash));
        fs::write(&path, "invalid checksum").unwrap();
        assert!(read_shasum(&path).is_err());
    }
}
