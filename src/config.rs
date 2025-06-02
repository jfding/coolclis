use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use reqwest::StatusCode;
use futures::stream::{FuturesUnordered, StreamExt};

#[derive(Debug, Deserialize, Serialize)]
pub struct CliTool {
    pub name: String,
    pub repo: String,
    pub description: String,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct CliToolsConfig {
    pub tools: Vec<CliTool>,
}

const DEFAULT_CONFIG: &str = include_str!("../data/cli-tools.json");

fn load_config_file() -> Result<CliToolsConfig> {
    let config_path = get_config_path()?;

    // Try to load from the current directory first
    let config_str = match fs::read_to_string(config_path) {
        Ok(content) => content,
        Err(_) => {
            // If not found in current directory, check for bundled file in the executable directory
            let exe_path = std::env::current_exe()?;
            let exe_dir = exe_path.parent().ok_or_else(|| anyhow!("Failed to determine executable directory"))?;
            let bundled_path = exe_dir.join("cli-tools.json");
            fs::read_to_string(bundled_path)?
        }
    };

    let config: CliToolsConfig = serde_json::from_str(&config_str)?;
    Ok(config)
}

fn get_config_path() -> Result<std::path::PathBuf> {
    let config_path = Path::new("cli-tools.json");

    // Check if the file exists in the current directory
    if config_path.exists() {
        return Ok(config_path.to_path_buf());
    }

    // If not found in current directory, use the file in .local/share/coolclis/
    let home_dir = dirs::home_dir().ok_or_else(|| anyhow!("Failed to determine home directory"))?;
    let config_path = home_dir.join(".local")
                                       .join("share")
                                       .join("coolclis")
                                       .join("cli-tools.json");

    if !config_path.exists() {
        fs::create_dir_all(config_path.parent().unwrap())?;
        fs::write(&config_path, DEFAULT_CONFIG)?;
    }

    Ok(config_path)
}

pub fn add_cli_tool(name: &str, repo: &str, description: &str) -> Result<()> {
    // Load existing config
    let mut config = load_config_file()?;

    // Check if tool with this name already exists
    if config.tools.iter().any(|tool| tool.name == name) {
        return Err(anyhow!("A tool with the name '{}' already exists", name));
    }

    // Add the new tool
    config.tools.push(CliTool {
        name: name.to_string(),
        repo: repo.to_string(),
        description: description.to_string(),
    });

    // Save the updated config
    let config_path = get_config_path()?;
    let json = serde_json::to_string_pretty(&config)?;
    fs::write(config_path, json)?;

    println!("Added tool '{}' ({}) to the configuration", name, repo);

    Ok(())
}

pub fn load_cli_tools() -> Result<HashMap<String, String>> {
    let config = load_config_file()?;

    // Create a map of tool names to repositories
    let mut tools_map = HashMap::new();
    for tool in config.tools {
        tools_map.insert(tool.name, tool.repo);
    }

    Ok(tools_map)
}

pub fn list_available_tools() -> Result<()> {
    let config = load_config_file()?;

    println!("Available CLI tools:");
    println!("{:<15} {:<30} DESCRIPTION", "NAME", "REPOSITORY");
    println!("{:<15} {:<30} -----------", "----", "----------");

    for tool in config.tools {
        println!("{:<15} {:<30} {}", tool.name, tool.repo, tool.description);
    }

    Ok(())
}

/// Checks if the GitHub repo for each tool is valid by sending a HEAD request to the releases/latest endpoint, in parallel.
pub async fn check_cli_tools_links_streaming() -> Result<()> {
    let config = load_config_file()?;
    let client = reqwest::Client::new();
    let mut futures = FuturesUnordered::new();

    for tool in config.tools {
        let client = client.clone();
        let name = tool.name.clone();
        let repo = tool.repo.clone();
        futures.push(async move {
            let url = format!("https://api.github.com/repos/{}/releases/latest", repo);
            let res = client
                .head(&url)
                .header("User-Agent", "curl")
                .send()
                .await;
            match res {
                Ok(resp) => {
                    if resp.status() == StatusCode::OK {
                        (name, repo, true, None)
                    } else {
                        (name, repo, false, Some(format!("HTTP {}", resp.status())))
                    }
                }
                Err(e) => {
                    (name, repo, false, Some(e.to_string()))
                }
            }
        });
    }

    println!("{:<15} {:<30} STATUS", "NAME", "REPOSITORY");
    println!("{:<15} {:<30} ------", "----", "----------");
    while let Some((name, repo, valid, err)) = futures.next().await {
        if valid {
            println!("{:<15} {:<30} OK", name, repo);
        } else {
            println!("{:<15} {:<30} INVALID: {}", name, repo, err.unwrap_or_else(|| "Unknown error".to_string()));
        }
    }
    Ok(())
}