use crate::error::SecurityError;
use reqwest::Client;
use semver::Version;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use anyhow::Result;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Vulnerability {
    pub id: String,
    pub title: String,
    pub description: String,
    pub severity: String,
    pub affected_versions: String,
    pub patched_version: Option<String>,
}

pub struct SecurityChecker {
    client: Client,
    cache: HashMap<String, Vec<Vulnerability>>,
}

impl SecurityChecker {
    pub fn new() -> Self {
        Self {
            client: Client::new(),
            cache: HashMap::new(),
        }
    }

    pub async fn check_package(&mut self, name: &str, version: &Version) -> Result<Vec<Vulnerability>> {
        // Query the NPM Security Advisory Database
        let url = format!(
            "https://registry.npmjs.org/-/npm/v1/security/advisories/search?package={}",
            name
        );

        let response = self.client.get(&url).send().await?;
        let vulnerabilities: Vec<Vulnerability> = response.json().await?;
        
        self.cache.insert(name.to_string(), vulnerabilities.clone());
        
        Ok(vulnerabilities)
    }

    pub async fn find_safe_version(
        &mut self,
        name: &str,
        current_version: &Version,
        available_versions: &[Version],
    ) -> Result<Version> {
        let vulnerabilities = self.check_package(name, current_version).await?;
        
        if vulnerabilities.is_empty() {
            return Ok(current_version.clone());
        }

        // Find the nearest safe version
        let safe_version = available_versions
            .iter()
            .rev()  // Start from newest versions
            .find(|&version| {
                !vulnerabilities.iter().any(|vuln| {
                    if let Some(patched) = &vuln.patched_version {
                        Version::parse(patched).map_or(true, |p| version >= &p)
                    } else {
                        false
                    }
                })
            })
            .ok_or_else(|| SecurityError::NoSafeVersion(name.to_string()))?;

        Ok(safe_version.clone())
    }
} 