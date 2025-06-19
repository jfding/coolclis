use anyhow::Result;
use std::fs;
use std::io::Cursor;
use std::path::{Path, PathBuf};

/// Extract an archive and find the executable within it
pub fn extract_archive(data: &[u8], filename: &str, dest_dir: &Path) -> Result<Option<PathBuf>> {
    let cursor = Cursor::new(data);

    if filename.ends_with(".tar.gz") || filename.ends_with(".tgz") {
        let tar = flate2::read::GzDecoder::new(cursor);
        let mut archive = tar::Archive::new(tar);
        archive.unpack(dest_dir)?;

        // Find executable files recursively
        find_executable_recursively(dest_dir)
    } else if filename.ends_with(".zip") {
        let mut archive = zip::ZipArchive::new(cursor)?;
        archive.extract(dest_dir)?;

        // Find executable files recursively
        find_executable_recursively(dest_dir)
    } else {
        // Not an archive, just a binary
        Ok(None)
    }
}

/// Find an executable file within a directory structure
pub fn find_executable_recursively(dir: &Path) -> Result<Option<PathBuf>> {
    let exe_name = dir.file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("")
        .strip_suffix("_temp")
        .unwrap_or("");

    // First, check if there's a bin directory with the executable
    let bin_dir = dir.join("bin");
    if bin_dir.exists() && bin_dir.is_dir() {
        for entry in fs::read_dir(&bin_dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_file() {
                // If we find the expected tool name in bin/, prioritize it
                if path.file_name()
                    .and_then(|n| n.to_str())
                    .map(|s| s == exe_name || s.starts_with(exe_name))
                    .unwrap_or(false)
                {
                    make_executable(&path)?;
                    return Ok(Some(path));
                }
            }
        }

        // If we didn't find an exact match, return the first file in bin/
        for entry in fs::read_dir(&bin_dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_file() {
                make_executable(&path)?;
                return Ok(Some(path));
            }
        }
    }

    // Next, try to find a file with the same name as the directory
    let possible_bin = dir.join(exe_name);
    if possible_bin.exists() && possible_bin.is_file() {
        make_executable(&possible_bin)?;
        return Ok(Some(possible_bin));
    }

    // Last, check for common executable names and locations
    let mut candidates = Vec::new();

    search_directory(dir, &mut candidates)?;

    // get the one with exe_name as the file name
    if let Some(exe_candidate) = candidates.iter()
        .find(|c| c.file_name().and_then(|n| n.to_str()).unwrap_or("") == exe_name) {

        make_executable(exe_candidate)?;
        return Ok(Some(exe_candidate.clone()));
    }

    // Sort candidates to prioritize likely executables
    candidates.sort_by(|a, b| {
        let a_name = a.file_name().and_then(|n| n.to_str()).unwrap_or("");
        let b_name = b.file_name().and_then(|n| n.to_str()).unwrap_or("");

        // Prioritize files without extensions
        let a_has_ext = a_name.contains('.');
        let b_has_ext = b_name.contains('.');

        if a_has_ext && !b_has_ext {
            std::cmp::Ordering::Greater
        } else if !a_has_ext && b_has_ext {
            std::cmp::Ordering::Less
        } else {
            // Secondary sort by name length (shorter names are likely commands)
            a_name.len().cmp(&b_name.len())
        }
    });

    // Take the first candidate after sorting, as the last fallback
    if let Some(path) = candidates.first() {
        make_executable(path)?;
        Ok(Some(path.clone()))
    } else {
        Ok(None)
    }
}

fn search_directory(dir: &Path, candidates: &mut Vec<PathBuf>) -> Result<()> {
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();

        if path.is_file() {
            let file_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");

            // Skip files that start with a dot or are LICENSE or README files
            if !file_name.starts_with('.') &&
                !file_name.starts_with("LICENSE") &&
                !file_name.starts_with("README") &&
                !file_name.contains(".md") &&
                !file_name.contains(".txt") {

                // Prioritize files without extensions
                if !file_name.contains('.') {
                    candidates.push(path.clone());
                } else {
                    candidates.push(path);
                }
            }
        } else if path.is_dir() {
            // Skip directories that start with a dot
            if path.file_name()
                .and_then(|n| n.to_str())
                .map(|s| !s.starts_with('.'))
                .unwrap_or(false)
            {
                // Check if this is a bin directory
                if path.file_name()
                    .and_then(|n| n.to_str())
                    .map(|s| s == "bin")
                    .unwrap_or(false)
                {
                    // Prioritize searching bin directories
                    search_directory(&path, candidates)?;
                } else {
                    search_directory(&path, candidates)?;
                }
            }
        }
    }
    Ok(())
}

/// Make a file executable on Unix systems
fn make_executable(path: &Path) -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let metadata = fs::metadata(path)?;
        let mut perms = metadata.permissions();
        perms.set_mode(0o755);
        fs::set_permissions(path, perms)?;
    }
    Ok(())
}
