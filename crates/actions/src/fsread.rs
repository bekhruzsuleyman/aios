//! `fs.read`: read a text file under one of the read roots. The path is
//! checked lexically (absolute or root-relative, no `..`), then
//! canonicalised (resolving every symlink) and checked again, so symlinks
//! cannot lead outside the roots. Reads are capped at 1 MB.

use std::io::Read;
use std::path::{Component, Path, PathBuf};

pub const MAX_BYTES: u64 = 1024 * 1024;

/// In-memory check used by the broker's decision (no disk access): the
/// path, resolved against the first root if relative, has no `..` and lies
/// under a root.
pub fn lexically_inside(roots: &[PathBuf], path: &Path) -> Option<PathBuf> {
    if path.components().any(|c| matches!(c, Component::ParentDir)) {
        return None;
    }
    let full = if path.is_absolute() {
        path.to_path_buf()
    } else {
        roots.first()?.join(path)
    };
    roots.iter().any(|r| full.starts_with(r)).then_some(full)
}

pub fn read(roots: &[PathBuf], path: &Path) -> Result<String, String> {
    let full = lexically_inside(roots, path).ok_or("path is outside the readable folders")?;
    let real = full
        .canonicalize()
        .map_err(|e| format!("cannot open {}: {e}", path.display()))?;
    let inside = roots
        .iter()
        .filter_map(|r| r.canonicalize().ok())
        .any(|r| real.starts_with(&r));
    if !inside {
        return Err("path leads outside the readable folders".into());
    }
    let meta = std::fs::metadata(&real).map_err(|e| e.to_string())?;
    if !meta.is_file() {
        return Err("not a regular file".into());
    }
    if meta.len() > MAX_BYTES {
        return Err(format!("file is larger than {} bytes", MAX_BYTES));
    }
    let mut buf = Vec::new();
    std::fs::File::open(&real)
        .map_err(|e| e.to_string())?
        .take(MAX_BYTES + 1)
        .read_to_end(&mut buf)
        .map_err(|e| e.to_string())?;
    if buf.len() as u64 > MAX_BYTES {
        return Err("file grew past the size limit".into());
    }
    String::from_utf8(buf).map_err(|_| "not a text file".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup(name: &str) -> (PathBuf, PathBuf) {
        let base = std::env::temp_dir().join(format!("aios-fsread-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let root = base.join("user");
        std::fs::create_dir_all(root.join("docs")).unwrap();
        std::fs::write(root.join("docs/todo.txt"), "buy milk").unwrap();
        std::fs::write(base.join("secret.txt"), "top secret").unwrap();
        (base, root)
    }

    #[test]
    fn reads_inside_roots() {
        let (base, root) = setup("ok");
        let roots = vec![root.clone()];
        assert_eq!(
            read(&roots, Path::new("docs/todo.txt")).unwrap(),
            "buy milk"
        );
        assert_eq!(
            read(&roots, &root.join("docs/todo.txt")).unwrap(),
            "buy milk"
        );
        std::fs::remove_dir_all(base).unwrap();
    }

    #[test]
    fn rejects_traversal_symlinks_and_huge_files() {
        let (base, root) = setup("bad");
        let roots = vec![root.clone()];
        assert!(read(&roots, Path::new("../secret.txt")).is_err());
        assert!(read(&roots, Path::new("/etc/passwd")).is_err());
        assert!(read(&roots, &root.join("docs/../../secret.txt")).is_err());
        std::os::unix::fs::symlink(base.join("secret.txt"), root.join("link")).unwrap();
        let err = read(&roots, Path::new("link")).unwrap_err();
        assert!(err.contains("outside"), "{err}");
        std::os::unix::fs::symlink(&base, root.join("dirlink")).unwrap();
        assert!(read(&roots, Path::new("dirlink/secret.txt")).is_err());
        let big = root.join("big.txt");
        std::fs::write(&big, vec![b'a'; (MAX_BYTES + 1) as usize]).unwrap();
        assert!(read(&roots, &big).unwrap_err().contains("larger"));
        assert!(read(&roots, Path::new("docs")).is_err());
        std::fs::remove_dir_all(base).unwrap();
    }

    #[test]
    fn lexical_check() {
        let roots = vec![PathBuf::from("/var/lib/aios/user")];
        assert!(lexically_inside(&roots, Path::new("a/b.txt")).is_some());
        assert!(lexically_inside(&roots, Path::new("/var/lib/aios/user/x")).is_some());
        assert!(lexically_inside(&roots, Path::new("/var/lib/aios/userx/y")).is_none());
        assert!(lexically_inside(&roots, Path::new("/etc/shadow")).is_none());
        assert!(lexically_inside(&roots, Path::new("a/../../b")).is_none());
    }
}
