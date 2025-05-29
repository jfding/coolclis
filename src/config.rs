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
    let config_path = Path::new("cli-tools.json");
    
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
    println!("{:<15} {:<30} {}", "NAME", "REPOSITORY", "DESCRIPTION");
    println!("{:<15} {:<30} {}", "----", "----------", "-----------");
    
    for tool in config.tools {
        println!("{:<15} {:<30} {}", tool.name, tool.repo, tool.description);
    }
    
    Ok(())
}