use crate::shared::fastzip;
use anyhow::{anyhow, bail, Result};
use std::{
    collections::HashSet,
    fs, io,
    path::{Path, PathBuf},
};

pub fn zstd_patch_single<P1: AsRef<Path>, P2: AsRef<Path>, P3: AsRef<Path>>(old_file: P1, patch_file: P2, output_file: P3) -> Result<()> {
    let old_file = old_file.as_ref();
    let patch_file = patch_file.as_ref();
    let output_file = output_file.as_ref();

    if !old_file.exists() {
        bail!("Old file does not exist: {:?}", old_file);
    }

    if !patch_file.exists() {
        bail!("Patch file does not exist: {:?}", patch_file);
    }

    let dict = fs::read(old_file)?;

    // info!("Loading Dictionary (Size: {})", dict.len());
    let patch = fs::OpenOptions::new().read(true).open(patch_file)?;
    let patch_reader = io::BufReader::new(patch);
    let mut decoder = zstd::Decoder::with_dictionary(patch_reader, &dict)?;

    let window_log = fio_highbit64(dict.len() as u64) + 1;
    if window_log >= 27 {
        info!("Large File detected. Overriding windowLog to {}", window_log);
        decoder.window_log_max(window_log)?;
    }

    // info!("Decoder loaded. Beginning patch...");
    let mut output = fs::OpenOptions::new().write(true).create(true).truncate(true).open(output_file)?;
    io::copy(&mut decoder, &mut output)?;

    // info!("Patch applied successfully.");
    Ok(())
}

fn fio_highbit64(v: u64) -> u32 {
    let mut count: u32 = 0;
    let mut v = v;
    v >>= 1;
    while v > 0 {
        v >>= 1;
        count += 1;
    }
    count
}

pub fn delta<P1: AsRef<Path>, P2: AsRef<Path>, P3: AsRef<Path>>(
    old_file: P1,
    delta_files: Vec<&PathBuf>,
    temp_dir: P2,
    output_file: P3,
) -> Result<()> {
    let old_file = old_file.as_ref().to_path_buf();
    let temp_dir = temp_dir.as_ref().to_path_buf();
    let output_file = output_file.as_ref().to_path_buf();

    if !old_file.exists() {
        bail!("Old file does not exist: {:?}", old_file);
    }

    if delta_files.is_empty() {
        bail!("No delta files provided.");
    }

    for delta_file in &delta_files {
        if !delta_file.exists() {
            bail!("Delta file does not exist: {:?}", delta_file);
        }
    }

    let time = simple_stopwatch::Stopwatch::start_new();

    info!("Extracting base package for delta patching: {:?}", temp_dir);
    let work_dir = temp_dir.join("_work");
    fs::create_dir_all(&work_dir)?;
    fastzip::extract_to_directory(old_file, &work_dir, None)?;

    info!("Base package extracted. {} delta packages to apply.", delta_files.len());

    for (i, delta_file) in delta_files.iter().enumerate() {
        info!("{}: extracting apply delta patch: {:?}", i, delta_file);
        let delta_dir = temp_dir.join(format!("delta_{}", i));
        fs::create_dir_all(&delta_dir)?;
        fastzip::extract_to_directory(delta_file, &delta_dir, None)?;

        let delta_relative_paths = fastzip::enumerate_files_relative(&delta_dir);
        let mut visited_paths = HashSet::new();

        // Apply current HDiffPatch and legacy zstd patches.
        for relative_path in &delta_relative_paths {
            if relative_path.starts_with("lib") {
                let file_name = relative_path.file_name().ok_or(anyhow!("Failed to get file name"))?;
                let file_name_str = file_name.to_string_lossy();
                // zero-length patches are "unchanged file" markers (always .diff). Non-empty .diff (msdelta)
                // and .bsdiff patches are legacy formats that vpk can no longer produce; they are still
                // matched here so a legacy delta fails loudly below instead of the patch file being
                // copied into the output package as if it were a new file.
                if file_name_str.ends_with(".hdiff")
                    || file_name_str.ends_with(".zsdiff")
                    || file_name_str.ends_with(".diff")
                    || file_name_str.ends_with(".bsdiff")
                {
                    let file_without_extension = relative_path.with_extension("");
                    let old_file_path = work_dir.join(&file_without_extension);
                    let patch_file_path = delta_dir.join(relative_path);
                    let output_file_path = delta_dir.join(&file_without_extension);

                    visited_paths.insert(file_without_extension);

                    if fs::metadata(&patch_file_path)?.len() == 0 && file_name_str.ends_with(".diff") {
                        if !old_file_path.is_file() {
                            bail!("Unchanged file is missing: {:?}", old_file_path);
                        }
                        // file has not changed, so we can continue.
                        continue;
                    }

                    if file_name_str.ends_with(".hdiff") {
                        info!("{}: applying HDiffPatch patch: {:?}", i, relative_path);
                        let (size, hash) = super::hdiff::read_shasum(&patch_file_path.with_extension("shasum"))?;
                        super::hdiff_patch_single(&old_file_path, &patch_file_path, &output_file_path, size, &hash)?;
                    } else if file_name_str.ends_with(".zsdiff") {
                        info!("{}: applying zsdiff patch: {:?}", i, relative_path);
                        zstd_patch_single(&old_file_path, &patch_file_path, &output_file_path)?;
                    } else {
                        bail!("Unsupported patch format: {:?}", relative_path);
                    }

                    fs::rename(&output_file_path, &old_file_path)?;
                } else if file_name_str.ends_with(".shasum") {
                    // skip shasum files
                } else {
                    // if this file is inside the lib folder without a known extension, it is a new file
                    let file_path = delta_dir.join(relative_path);
                    let dest_path = work_dir.join(relative_path);
                    info!("{}: new file: {:?}", i, relative_path);
                    fs::create_dir_all(dest_path.parent().ok_or(anyhow!("Failed to get parent"))?)?;
                    fs::copy(&file_path, &dest_path)?;
                    visited_paths.insert(relative_path.clone());
                }
            } else {
                // if this file is not inside the lib folder, we always copy it over
                let file_path = delta_dir.join(relative_path);
                let dest_path = work_dir.join(relative_path);
                info!("{}: copying metadata file: {:?}", i, relative_path);
                fs::copy(&file_path, &dest_path)?;
                visited_paths.insert(relative_path.clone());
            }
        }

        // anything in the work dir which was not visited is an old / deleted file and should be removed
        let workdir_relative_paths = fastzip::enumerate_files_relative(&work_dir);
        for relative_path in &workdir_relative_paths {
            if !visited_paths.contains(relative_path) {
                let file_to_delete = work_dir.join(relative_path);
                info!("{}: deleting old/removed file: {:?}", i, relative_path);
                let _ = fs::remove_file(file_to_delete); // soft error
            }
        }
    }

    info!("All delta patches applied. Asembling output package at: {:?}", output_file);

    fastzip::compress_directory(&work_dir, &output_file)?;

    info!("Successfully applied {} delta patches in {}s.", delta_files.len(), time.s());
    Ok(())
}

#[cfg(test)]
mod hdiff_package_tests {
    use super::*;

    const OLD: &[u8] = include_bytes!("../../../../test/fixtures/hdiffpatch/old.bin");
    const NEW: &[u8] = include_bytes!("../../../../test/fixtures/hdiffpatch/new.bin");

    fn put(root: &Path, relative: &str, bytes: &[u8]) {
        let path = root.join(relative);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, bytes).unwrap();
    }

    #[test]
    fn applies_hdiff_packages_with_added_removed_and_unchanged_files() {
        let temp = tempfile::tempdir().unwrap();
        let base = temp.path().join("base");
        let delta_dir = temp.path().join("delta");
        put(&base, "lib/app/changed", OLD);
        put(&base, "lib/app/same", b"unchanged");
        put(&base, "lib/app/deleted", b"deleted");
        put(&base, "test.nuspec", b"version 1");
        put(&delta_dir, "lib/app/changed.hdiff", include_bytes!("../../../../test/fixtures/hdiffpatch/speed.hdiff"));
        let hash = sha1_smol::Sha1::from(NEW).digest().to_string();
        put(&delta_dir, "lib/app/changed.shasum", format!("\u{feff}{hash} changed.shasum {}", NEW.len()).as_bytes());
        put(&delta_dir, "lib/app/same.diff", b"");
        put(&delta_dir, "lib/app/same.shasum", b"");
        put(&delta_dir, "lib/app/added", b"added");
        put(&delta_dir, "test.nuspec", b"version 2");
        let base_zip = temp.path().join("base.zip");
        let delta_zip = temp.path().join("delta.zip");
        fastzip::compress_directory(&base, &base_zip).unwrap();
        fastzip::compress_directory(&delta_dir, &delta_zip).unwrap();
        let output = temp.path().join("result.zip");
        delta(&base_zip, vec![&delta_zip], temp.path().join("work"), &output).unwrap();
        let restored = temp.path().join("restored");
        fastzip::extract_to_directory(output, &restored, None).unwrap();
        assert_eq!(fs::read(restored.join("lib/app/changed")).unwrap(), NEW);
        assert_eq!(fs::read(restored.join("lib/app/same")).unwrap(), b"unchanged");
        assert_eq!(fs::read(restored.join("lib/app/added")).unwrap(), b"added");
        assert_eq!(fs::read(restored.join("test.nuspec")).unwrap(), b"version 2");
        assert_eq!(fastzip::enumerate_files_relative(restored).len(), 4);
    }

    #[test]
    fn rejects_empty_hdiff_and_missing_checksum() {
        for patch in [b"".as_slice(), include_bytes!("../../../../test/fixtures/hdiffpatch/speed.hdiff").as_slice()] {
            let temp = tempfile::tempdir().unwrap();
            let base = temp.path().join("base");
            let delta_dir = temp.path().join("delta");
            put(&base, "lib/app/changed", OLD);
            put(&delta_dir, "lib/app/changed.hdiff", patch);
            let base_zip = temp.path().join("base.zip");
            let delta_zip = temp.path().join("delta.zip");
            fastzip::compress_directory(&base, &base_zip).unwrap();
            fastzip::compress_directory(&delta_dir, &delta_zip).unwrap();
            assert!(delta(&base_zip, vec![&delta_zip], temp.path().join("work"), temp.path().join("result.zip")).is_err());
        }
    }
}
