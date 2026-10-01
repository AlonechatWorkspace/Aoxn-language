//! Package tarball (transport format) and the manifest hash (integrity).
//!
//! **Integrity does NOT live in tarball bytes.** The checksum stored in the
//! registry index and pinned in `aoxn.lock` is a *manifest hash* over a
//! normalized file listing:
//!
//! ```text
//! checksum = sha256( concat( "<posix_rel_path>\0<sha256(content)>\n" ) for f in sorted(files) )
//! ```
//!
//! This is immune to tar/gzip implementation quirks, platform differences
//! and re-packing; `install` verifies it by recomputing over the extracted
//! tree, which is exactly the supply-chain property we want (the bytes you
//! run are what the checksum describes). The tarball is merely transport —
//! but we still normalize it best-effort (sorted walk, mtime=0, uid/gid=0,
//! fixed modes, gzip mtime=0) so re-packs of the same tree are byte-stable.
//!
//! Excluded from packages: `target/`, `aox_modules/`, `.git/`, `aoxn.lock`,
//! `*.tar.gz` (so packing never includes its own output).

use std::fs::File;
use std::path::{Component, Path, PathBuf};

use crate::cache::sha256_hex;
use crate::errors::PkgError;

/// Directories/files never included in a published tarball.
fn excluded(relative: &Path) -> bool {
    for comp in relative.components() {
        let Some(s) = comp.as_os_str().to_str() else {
            return true; // non-UTF-8 path component: skip
        };
        if matches!(
            s,
            "target" | "aox_modules" | ".git" | ".aoxn" | ".hg" | ".svn"
        ) && comp == Component::Normal(std::ffi::OsStr::new(s))
        {
            return true;
        }
    }
    let file_name = relative.file_name().map(|f| f.to_string_lossy().to_string());
    match file_name.as_deref() {
        Some("aoxn.lock") => true,
        Some(f) => f.ends_with(".tar.gz"),
        None => false,
    }
}

/// POSIX-style relative path (forward slashes), for deterministic keys.
fn rel_posix(rel: &Path) -> String {
    rel.to_string_lossy().replace('\\', "/")
}

/// Sorted list of (posix rel path, absolute path) for every packed file.
pub fn list_files(pkg_dir: &Path) -> Result<Vec<(String, PathBuf)>, PkgError> {
    let mut out = Vec::new();
    fn walk(dir: &Path, rel: &Path, out: &mut Vec<(String, PathBuf)>) -> Result<(), PkgError> {
        let full = dir.join(rel);
        if full.is_dir() {
            if !rel.as_os_str().is_empty() && excluded(rel) {
                return Ok(());
            }
            let mut entries: Vec<_> = std::fs::read_dir(&full)?.collect::<Result<Vec<_>, _>>()?;
            entries.sort_by_key(|e| e.file_name());
            for e in entries {
                let next = rel.join(e.file_name());
                walk(dir, &next, out)?;
            }
        } else if !excluded(rel) {
            out.push((rel_posix(rel), full));
        }
        Ok(())
    }
    walk(pkg_dir, Path::new(""), &mut out)?;
    Ok(out)
}

/// The manifest hash: sha256 over a normalized file listing (see module
/// docs). Computed at publish time, stored in the registry index, pinned in
/// the lockfile, re-verified over the extracted tree at install time.
pub fn manifest_hash(pkg_dir: &Path) -> Result<String, PkgError> {
    let files = list_files(pkg_dir)?;
    let mut acc = String::new();
    for (rel, abs) in &files {
        let content = std::fs::read(abs)?;
        acc.push_str(&format!("{rel}\0{}\n", sha256_hex(&content)));
    }
    Ok(sha256_hex(acc.as_bytes()))
}

/// Pack `pkg_dir` into `out_path` (tar.gz, entries rooted at `pkg_name`).
/// Headers are normalized (mtime/uid/gid zeroed, fixed modes, sorted walk,
/// gzip mtime=0) so identical trees re-pack to identical bytes. Returns the
/// number of files packed.
pub fn pack(pkg_dir: &Path, pkg_name: &str, out_path: &Path) -> Result<usize, PkgError> {
    let files = list_files(pkg_dir)?;
    if let Some(parent) = out_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let out = File::create(out_path)?;
    // gzip header without a timestamp (byte-stable re-packs)
    let gz = flate2::GzBuilder::new().mtime(0).write(out, flate2::Compression::default());
    let mut builder = tar::Builder::new(gz);

    // directory entries first (sorted), then files
    let mut dirs: Vec<String> = Vec::new();
    for (rel, _) in &files {
        let mut p = Path::new(rel);
        while let Some(parent) = p.parent() {
            if parent.as_os_str().is_empty() {
                break;
            }
            let ps = rel_posix(parent);
            if !dirs.contains(&ps) {
                dirs.push(ps.clone());
            }
            p = parent;
        }
    }
    dirs.sort();
    dirs.dedup();
    for d in &dirs {
        let mut header = tar::Header::new_gnu();
        header.set_entry_type(tar::EntryType::Directory);
        header.set_size(0);
        header.set_mode(0o755);
        header.set_mtime(0);
        header.set_uid(0);
        header.set_gid(0);
        header.set_cksum();
        let arc = format!("{pkg_name}/{d}");
        builder.append_data(&mut header, arc, std::io::empty())?;
    }
    for (rel, abs) in &files {
        let content = std::fs::read(abs)?;
        let mut header = tar::Header::new_gnu();
        header.set_size(content.len() as u64);
        header.set_mode(0o644);
        header.set_mtime(0);
        header.set_uid(0);
        header.set_gid(0);
        header.set_cksum();
        let arc = format!("{pkg_name}/{rel}");
        builder.append_data(&mut header, arc, content.as_slice())?;
    }
    builder.finish()?;
    Ok(files.len())
}

/// Extract a tarball into `dest` (created if missing). All entries must be
/// rooted at `pkg_name/` and stay inside `dest` — anything else is rejected
/// as malformed (and treated as a potential supply-chain attack).
pub fn unpack(tarball: &Path, dest: &Path, pkg_name: &str) -> Result<(), PkgError> {
    std::fs::create_dir_all(dest)?;
    let file = File::open(tarball)?;
    let gz = flate2::read::GzDecoder::new(file);
    let mut archive = tar::Archive::new(gz);
    let root = format!("{pkg_name}/");
    for entry in archive.entries()? {
        let mut entry = entry?;
        let path = entry.path()?.to_path_buf();
        let Some(stripped) = path.to_str().and_then(|p| p.strip_prefix(&root)) else {
            return Err(PkgError::Registry(format!(
                "malformed package tarball: entry `{}` is not rooted at `{root}`",
                path.display()
            )));
        };
        if stripped.contains("..") {
            return Err(PkgError::Registry(format!(
                "malformed package tarball: entry `{}` escapes the package directory",
                path.display()
            )));
        }
        let target = dest.join(stripped.replace('/', std::path::MAIN_SEPARATOR_STR));
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)?;
        }
        entry.unpack(&target)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(p: &Path, content: &str) {
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, content).unwrap();
    }

    fn temp_root(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("aoxn-pkg-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn pack_unpack_roundtrip_excludes_artifacts() {
        let tmp = temp_root("test");
        write(&tmp.join("src").join("main.ax"), "def main(): pass\n");
        write(&tmp.join("src").join("aoxn.json"), "{}");
        write(&tmp.join("target").join("cache").join("junk"), "no");
        write(&tmp.join("aox_modules").join("dep").join("junk.ax"), "no");

        let tgz = tmp.join("out.tar.gz");
        let n = pack(&tmp, "mypkg", &tgz).unwrap();
        assert!(n >= 2);

        let dest = tmp.join("unpacked");
        unpack(&tgz, &dest, "mypkg").unwrap();
        assert!(dest.join("src").join("main.ax").exists());
        assert!(dest.join("src").join("aoxn.json").exists());
        assert!(!dest.join("target").exists());
        assert!(!dest.join("aox_modules").exists());
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn manifest_hash_is_content_and_layout_sensitive() {
        let a = temp_root("hash-a");
        write(&a.join("lib.ax"), "def f(): pass\n");
        let h1 = manifest_hash(&a).unwrap();

        // same content, same name -> same hash
        let b = temp_root("hash-b");
        write(&b.join("lib.ax"), "def f(): pass\n");
        assert_eq!(h1, manifest_hash(&b).unwrap());

        // changed content -> different hash
        write(&a.join("lib.ax"), "def f(): pass # changed\n");
        assert_ne!(h1, manifest_hash(&a).unwrap());

        // same content, different layout -> different hash
        let c = temp_root("hash-c");
        write(&c.join("other.ax"), "def f(): pass\n");
        assert_ne!(h1, manifest_hash(&c).unwrap());

        let _ = std::fs::remove_dir_all(&a);
        let _ = std::fs::remove_dir_all(&b);
        let _ = std::fs::remove_dir_all(&c);
    }

    #[test]
    fn extracted_tree_matches_manifest_hash() {
        // the install-time guarantee: hash(extracted) == hash(packed tree)
        let tmp = temp_root("verify");
        write(&tmp.join("src").join("main.ax"), "def main(): pass\n");
        write(&tmp.join("src").join("util.ax"), "def u(): pass\n");
        let before = manifest_hash(&tmp).unwrap();
        let tgz = tmp.join("out.tar.gz");
        pack(&tmp, "mypkg", &tgz).unwrap();
        let dest = tmp.join("unpacked");
        unpack(&tgz, &dest, "mypkg").unwrap();
        let after = manifest_hash(&dest).unwrap();
        assert_eq!(before, after, "extracted tree must hash identically");
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn pack_is_byte_stable() {
        let tmp = temp_root("stable");
        write(&tmp.join("b.ax"), "b\n");
        write(&tmp.join("a.ax"), "a\n");
        write(&tmp.join("sub").join("c.ax"), "c\n");
        let t1 = tmp.join("p1.tar.gz");
        let t2 = tmp.join("p2.tar.gz");
        pack(&tmp, "mypkg", &t1).unwrap();
        pack(&tmp, "mypkg", &t2).unwrap();
        assert_eq!(
            crate::cache::sha256_file(&t1).unwrap(),
            crate::cache::sha256_file(&t2).unwrap(),
            "two packs of the same tree must be byte-identical"
        );
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn traversal_entry_is_rejected() {
        // hand-craft a tar header with a `..` entry name (tar-rs refuses to
        // write such paths itself, so the bytes are built by hand)
        let tmp = temp_root("evil");
        let tgz = tmp.join("evil.tar.gz");
        {
            let f = File::create(&tgz).unwrap();
            let mut enc = flate2::write::GzEncoder::new(f, flate2::Compression::default());
            let mut block = [0u8; 512];
            block[..11].copy_from_slice(b"../evil.txt");
            // size = 2 (octal, NUL-terminated), field at offset 124
            block[124..136].copy_from_slice(b"00000000002\x00");
            block[156] = b'0'; // regular file
            // cksum = sum of all header bytes with the cksum field as spaces
            let mut sum: u32 = 0;
            for (i, b) in block.iter().enumerate() {
                sum += if (148..156).contains(&i) { 32 } else { *b as u32 };
            }
            block[148..156].copy_from_slice(format!("{sum:06o}\x00 ").as_bytes());
            use std::io::Write;
            enc.write_all(&block).unwrap();
            enc.write_all(b"oh").unwrap();
            enc.write_all(&[0u8; 512]).unwrap(); // end-of-archive
            enc.finish().unwrap();
        }
        let dest = tmp.join("out");
        let err = unpack(&tgz, &dest, "mypkg").unwrap_err();
        assert!(format!("{err}").contains("malformed"), "got: {err}");
        assert!(!dest.join("evil.txt").exists());
        let _ = std::fs::remove_dir_all(&tmp);
    }
}
