use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::Path;

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
    let config_base = home_dir.join(".local")
                                       .join("share")
                                       .join("coolclis");

    if !config_base.exists() {
        fs::create_dir_all(&config_base)?;
    }

    let config_path = config_base.join("cli-tools.json");
    if !config_path.exists() {
        // Create the directory if it doesn't exist
        fs::create_dir_all(&config_base)?;

        // Create the config file if it doesn't exist
        let default_config = CliToolsConfig {
            tools: Vec::new(),
        };
        let json = serde_json::to_string_pretty(&default_config)?;
        fs::write(&config_path, json)?;
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