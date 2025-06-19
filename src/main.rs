use anyhow::{anyhow, Result};
use clap::{Parser, Subcommand};
use serde::{Deserialize, Serialize};
use std::fs::{self, File};
use std::io::{self, Cursor};
use std::path::PathBuf;

mod downloader;
use downloader::Downloader;

mod config;
use config::{load_cli_tools, list_available_tools, add_cli_tool, check_cli_tools_links_streaming};

mod unpack;
use unpack::extract_archive;

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
        tool: String,

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
        /// GitHub repository in the format owner/repo
        repo: String,

        /// Tool name (used for the executable name and as an identifier, defaults to extract from the repo name)
        #[arg(short, long)]
        name: Option<String>,

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
        vec!["apple-darwin", "darwin", "macos", "mac", "osx"]
    } else if os == "windows" {
        vec!["pc-windows", "windows"]
    } else if os == "linux" {
        vec!["unknown-linux", "linux"]
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
            search_patterns.push(format!("{}-{}", arch_var, os_var));
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

async fn install_tool(repo: &str, version: Option<&str>, dir: Option<&PathBuf>) -> Result<()> {
    let tool = repo.split('/').next_back().unwrap();

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
        Commands::Install { tool, version, dir } => {
            // Load the tools map
            let tools_map = load_cli_tools()?;

            // Check if the repo is a known tool name
            let actual_repo = if tool.contains('/') {
                tool.to_string()
            } else {
                tools_map.get(tool)
                    .ok_or_else(|| anyhow!("Unknown tool: {}. Use the 'list' command to see available tools.", tool))?
                    .to_string()
            };

            install_tool(&actual_repo, version.as_deref(), dir.as_ref()).await?;
        },
        Commands::List => {
            list_available_tools()?;
        },
        Commands::Add { repo, name, description } => {
            // Validate repository format
            if !repo.contains('/') || repo.matches('/').count() != 1 {
                return Err(anyhow!("Repository must be in the format 'owner/repo'"));
            }

            // Use a default name if none provided
            let tool = name.as_deref().unwrap_or(repo.split('/').next_back().unwrap());

            // Use a default description if none provided
            let desc = description.as_deref().unwrap_or("No description provided");

            add_cli_tool(tool, repo, desc)?;
        },
        Commands::Check => {
            check_cli_tools_links_streaming().await?;
        }
    }

    Ok(())
}
