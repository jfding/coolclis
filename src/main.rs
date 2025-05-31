use anyhow::{anyhow, Result};
use clap::{Parser, Subcommand};
use serde::{Deserialize, Serialize};
use std::fs::{self, File};
use std::io::{self, Cursor};
use std::path::{Path, PathBuf};

mod downloader;
use downloader::Downloader;

mod config;
use config::{load_cli_tools, list_available_tools, add_cli_tool, check_cli_tools_links_streaming};

#[derive(Parser)]
#[command(name = "coolclis")]
#[command(about = "A tool to download and install CLI tools from GitHub releases", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Install a tool from GitHub
    Install {
        /// GitHub repository in the format owner/repo or a predefined tool name
        repo: String,

        /// Tool name (used for the executable name)
        #[arg(short, long)]
        bin: Option<String>,

        /// Specific version to install (defaults to latest)
        #[arg(short, long)]
        version: Option<String>,

        /// Installation directory (defaults to ~/.local/bin)
        #[arg(short, long)]
        dir: Option<PathBuf>,
    },

    /// List all available predefined tools
    List,

    /// Add a new tool to the configuration
    Add {
        /// Tool name (used for the executable name and as an identifier)
        name: String,

        /// GitHub repository in the format owner/repo
        repo: String,

        /// Description of the tool
        #[arg(short, long)]
        description: Option<String>,
    },

    /// Check all tool links in the config file (validate GitHub repo exists)
    Check,
}

#[derive(Debug, Deserialize, Serialize)]
struct Release {
    tag_name: String,
    assets: Vec<Asset>,
}

#[derive(Debug, Deserialize, Serialize)]
struct Asset {
    name: String,
    browser_download_url: String,
    size: u64,
}
async fn get_latest_release(repo: &str) -> Result<Release> {
    let downloader = Downloader::default();
    let url = format!("https://api.github.com/repos/{}/releases/latest", repo);
    downloader.get_json::<Release>(&url).await
}

async fn get_specific_release(repo: &str, version: &str) -> Result<Release> {
    let downloader = Downloader::default();
    let url = format!(
        "https://api.github.com/repos/{}/releases/tags/{}",
        repo, version
    );
    downloader.get_json::<Release>(&url).await
}

fn get_platform_info() -> (String, String) {
    let os = if cfg!(target_os = "windows") {
        "windows"
    } else if cfg!(target_os = "macos") {
        "darwin"
    } else if cfg!(target_os = "linux") {
        "linux"
    } else {
        "unknown"
    };

    let arch = if cfg!(target_arch = "x86_64") {
        "x86_64"
    } else if cfg!(target_arch = "aarch64") {
        "arm64"
    } else if cfg!(target_arch = "x86") {
        "x86"
    } else {
        "unknown"
    };

    (os.to_string(), arch.to_string())
}

fn find_appropriate_asset<'a>(release: &'a Release, tool_name: &str) -> Result<&'a Asset> {
    let (os, arch) = get_platform_info();

    // Variations of OS/arch in filenames
    let os_variations: Vec<&str> = if os == "darwin" {
        vec!["darwin", "macos", "mac", "osx"]
    } else {
        vec![&os]
    };

    let arch_variations: Vec<&str> = if arch == "x86_64" {
        vec!["x86_64", "amd64", "x64"]
    } else if arch == "arm64" {
        vec!["arm64", "aarch64"]
    } else {
        vec![&arch]
    };

    // Create combinations of search terms
    let mut search_patterns = Vec::new();
    for os_var in &os_variations {
        for arch_var in &arch_variations {
            search_patterns.push(format!("{}-{}", os_var, arch_var));
            search_patterns.push(format!("{}_{}", os_var, arch_var));
            search_patterns.push(format!("{}{}", os_var, arch_var));
        }
        search_patterns.push(os_var.to_string()); // OS only pattern
    }

    // Extensions to look for
    let extensions = if os == "windows" {
        vec![".exe", ".zip", ".tar.gz", ".tgz"]
    } else {
        vec!["", ".tar.gz", ".tgz", ".zip"]
    };

    // First try to find assets that match the tool name
    for pattern in &search_patterns {
        for ext in &extensions {
            for asset in &release.assets {
                let name = asset.name.to_lowercase();
                let tool_lower = tool_name.to_lowercase();

                if name.contains(&tool_lower) && name.contains(pattern) && name.ends_with(ext) {
                    return Ok(asset);
                }
            }
        }
    }

    // If we couldn't find an asset matching the tool name, try to find any asset for the platform
    for pattern in &search_patterns {
        for ext in &extensions {
            for asset in &release.assets {
                let name = asset.name.to_lowercase();
                if name.contains(pattern) && name.ends_with(ext) {
                    return Ok(asset);
                }
            }
        }
    }

    Err(anyhow!("No suitable asset found for your platform ({}-{})", os, arch))
}

fn extract_archive(data: &[u8], filename: &str, dest_dir: &Path) -> Result<Option<PathBuf>> {
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

fn find_executable_recursively(dir: &Path) -> Result<Option<PathBuf>> {
    // First, try to find a file with the same name as the last directory component
    if let Some(dir_name) = dir.file_name().and_then(|n| n.to_str()) {
        // Check if there's a bin directory with the executable
        let bin_dir = dir.join("bin");
        if bin_dir.exists() && bin_dir.is_dir() {
            for entry in fs::read_dir(&bin_dir)? {
                let entry = entry?;
                let path = entry.path();
                if path.is_file() {
                    // If we find the expected tool name in bin/, prioritize it
                    if path.file_name()
                        .and_then(|n| n.to_str())
                        .map(|s| s == dir_name || s.starts_with(dir_name))
                        .unwrap_or(false)
                    {
                        #[cfg(unix)]
                        {
                            use std::os::unix::fs::PermissionsExt;
                            let metadata = fs::metadata(&path)?;
                            let mut perms = metadata.permissions();
                            perms.set_mode(0o755);
                            fs::set_permissions(&path, perms)?;
                        }
                        return Ok(Some(path));
                    }
                }
            }

            // If we didn't find an exact match, return the first file in bin/
            for entry in fs::read_dir(&bin_dir)? {
                let entry = entry?;
                let path = entry.path();
                if path.is_file() {
                    #[cfg(unix)]
                    {
                        use std::os::unix::fs::PermissionsExt;
                        let metadata = fs::metadata(&path)?;
                        let mut perms = metadata.permissions();
                        perms.set_mode(0o755);
                        fs::set_permissions(&path, perms)?;
                    }
                    return Ok(Some(path));
                }
            }
        }

        // Next, try to find a file with the same name as the directory
        let possible_bin = dir.join(dir_name);
        if possible_bin.exists() && possible_bin.is_file() {
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let metadata = fs::metadata(&possible_bin)?;
                let mut perms = metadata.permissions();
                perms.set_mode(0o755);
                fs::set_permissions(&possible_bin, perms)?;
            }
            return Ok(Some(possible_bin));
        }
    }

    // Check for common executable names and locations
    let mut candidates = Vec::new();

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

    search_directory(dir, &mut candidates)?;

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

    // Take the first candidate
    if let Some(path) = candidates.first() {
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let metadata = fs::metadata(path)?;
            let mut perms = metadata.permissions();
            perms.set_mode(0o755);
            fs::set_permissions(path, perms)?;
        }

        Ok(Some(path.clone()))
    } else {
        Ok(None)
    }
}

async fn install_tool(repo: &str, bin: Option<&str>, version: Option<&str>, dir: Option<&PathBuf>) -> Result<()> {
    let tool = bin.unwrap_or(repo.split('/').last().unwrap());

    println!("Installing {} from {}", tool, repo);

    // Get the release
    let release = match version {
        Some(v) => get_specific_release(repo, v).await?,
        None => get_latest_release(repo).await?,
    };

    println!("Found release: {}", release.tag_name);

    // Find the right asset
    let asset = find_appropriate_asset(&release, tool)?;
    println!("Selected asset: {} ({} bytes)", asset.name, asset.size);

    // Download the asset
    let downloader = Downloader::default();
    let data = downloader.download_file(&asset.browser_download_url, asset.size).await?;

    // Determine install directory
    let install_dir = match dir {
        Some(d) => d.clone(),
        None => {
            let mut home_dir = dirs::home_dir().ok_or_else(|| anyhow!("Could not determine home directory"))?;
            home_dir.push(".local");
            home_dir.push("bin");
            fs::create_dir_all(&home_dir)?;
            home_dir
        }
    };

    // Create a temporary directory for extraction if needed
    let temp_dir = install_dir.join(format!("{}_temp", tool));
    if temp_dir.exists() {
        fs::remove_dir_all(&temp_dir)?;
    }
    fs::create_dir_all(&temp_dir)?;

    // Check if the downloaded file is an archive that needs extraction
    let file_path = if asset.name.ends_with(".tar.gz") || asset.name.ends_with(".tgz") || asset.name.ends_with(".zip") {
        println!("Extracting archive...");

        // Extract the archive
        let extracted_path = extract_archive(&data, &asset.name, &temp_dir)?;

        // Move the extracted binary to the final location
        match extracted_path {
            Some(path) => {
                println!("Found executable: {}", path.display());
                let dest_path = install_dir.join(tool);
                fs::copy(path, &dest_path)?;
                dest_path
            },
            None => {
                return Err(anyhow!("Could not find executable in extracted archive"));
            }
        }
    } else {
        // It's a direct binary
        let file_path = install_dir.join(tool);
        let mut file = File::create(&file_path)?;
        io::copy(&mut Cursor::new(data), &mut file)?;

        // Make the file executable on Unix systems
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let metadata = fs::metadata(&file_path)?;
            let mut perms = metadata.permissions();
            perms.set_mode(0o755);
            fs::set_permissions(&file_path, perms)?;
        }

        file_path
    };

    // Clean up the temporary directory
    fs::remove_dir_all(temp_dir)?;

    println!("Successfully installed {} to {}", tool, file_path.display());
    println!("Make sure {} is in your PATH", install_dir.display());

    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    match &cli.command {
        Commands::Install { repo, bin, version, dir } => {
            // Load the tools map
            let tools_map = load_cli_tools()?;

            // Check if the repo is a known tool name
            let actual_repo = if repo.contains('/') {
                repo.to_string()
            } else {
                tools_map.get(repo)
                    .ok_or_else(|| anyhow!("Unknown tool: {}. Use the 'list' command to see available tools.", repo))?
                    .to_string()
            };

            install_tool(&actual_repo, bin.as_deref(), version.as_deref(), dir.as_ref()).await?;
        },
        Commands::List => {
            list_available_tools()?;
        },
        Commands::Add { name, repo, description } => {
            // Validate repository format
            if !repo.contains('/') || repo.matches('/').count() != 1 {
                return Err(anyhow!("Repository must be in the format 'owner/repo'"));
            }

            // Use a default description if none provided
            let desc = description.as_deref().unwrap_or("No description provided");

            add_cli_tool(name, repo, desc)?;
        },
        Commands::Check => {
            check_cli_tools_links_streaming().await?;
        }
    }

    Ok(())
}
