use anyhow::{anyhow, Context, Result};
use indicatif::{ProgressBar, ProgressStyle};
use reqwest::{header::{HeaderMap, HeaderValue, USER_AGENT}, StatusCode};
use std::time::Duration;

pub struct Downloader {
    client: reqwest::Client,
    max_attempts: usize,
    retry_delay_secs: u64,
}

impl Default for Downloader {
    fn default() -> Self {
        Self::new(3, 120, 2)
    }
}

impl Downloader {
    pub fn new(max_attempts: usize, timeout_secs: u64, retry_delay_secs: u64) -> Self {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(timeout_secs))
            .build()
            .expect("Failed to build HTTP client");

        Self {
            client,
            max_attempts,
            retry_delay_secs,
        }
    }

    pub async fn get_json<T: serde::de::DeserializeOwned>(&self, url: &str) -> Result<T> {
        let mut headers = HeaderMap::new();
        headers.insert(USER_AGENT, HeaderValue::from_static("coolclis"));

        let mut attempts = 0;

        while attempts < self.max_attempts {
            attempts += 1;

            match self.client.get(url).headers(headers.clone()).send().await {
                Ok(response) => {
                    if response.status() == StatusCode::NOT_FOUND {
                        return Err(anyhow!("{} not found (404)", url));
                    }
                    if response.status().is_success() {
                        match response.json::<T>().await {
                            Ok(data) => return Ok(data),
                            Err(e) => {
                                if attempts < self.max_attempts {
                                    println!("Failed to parse JSON (attempt {}): {}", attempts, e);
                                    tokio::time::sleep(Duration::from_secs(self.retry_delay_secs)).await;
                                } else {
                                    return Err(anyhow!("Failed to parse JSON: {}", e));
                                }
                            }
                        }
                    } else if attempts < self.max_attempts {
                        println!("Failed to fetch URL (attempt {}): {}", attempts, response.status());
                        tokio::time::sleep(Duration::from_secs(self.retry_delay_secs)).await;
                    } else {
                        return Err(anyhow!("Failed to fetch URL: {}", response.status()));
                    }
                },
                Err(e) => {
                    if attempts < self.max_attempts {
                        println!("Failed to send request (attempt {}): {}", attempts, e);
                        tokio::time::sleep(Duration::from_secs(self.retry_delay_secs)).await;
                    } else {
                        return Err(anyhow!("Failed to send request: {}", e));
                    }
                }
            }
        }

        Err(anyhow!("Failed to fetch URL after {} attempts", self.max_attempts))
    }

    pub async fn download_file(&self, url: &str, size: u64) -> Result<Vec<u8>> {
        let pb = ProgressBar::new(size);
        pb.set_style(
            ProgressStyle::default_bar()
                .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {bytes}/{total_bytes} ({eta})")
                .unwrap()
                .progress_chars("#>-"),
        );

        let mut attempts = 0;

        while attempts < self.max_attempts {
            attempts += 1;

            match self.download_attempt(url, &pb).await {
                Ok(buffer) => {
                    pb.finish_with_message("Download complete");
                    return Ok(buffer);
                }
                Err(e) => {
                    if attempts < self.max_attempts {
                        println!("Download attempt {} failed: {}", attempts, e);
                        println!("Retrying in {} seconds...", self.retry_delay_secs);
                        tokio::time::sleep(Duration::from_secs(self.retry_delay_secs)).await;
                    } else {
                        pb.finish_with_message("Download failed");
                        return Err(anyhow!("Failed to download file after {} attempts: {}", self.max_attempts, e));
                    }
                }
            }
        }

        Err(anyhow!("Failed to download file"))
    }

    async fn download_attempt(&self, url: &str, pb: &ProgressBar) -> Result<Vec<u8>> {
        let mut response = self.client.get(url)
            .header(USER_AGENT, "coolclis")
            .send()
            .await
            .context("Failed to send download request")?;

        if !response.status().is_success() {
            return Err(anyhow!("Failed to download: HTTP status {}", response.status()));
        }

        let mut buffer = Vec::new();
        let mut downloaded: u64 = 0;

        while let Some(chunk) = response.chunk().await.context("Failed to download chunk")? {
            downloaded += chunk.len() as u64;
            pb.set_position(downloaded);
            buffer.extend_from_slice(&chunk);
        }

        Ok(buffer)
    }
}