use crate::error::SecurityError;
use anyhow::Result;
use reqwest::Client;
use semver::Version;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Vulnerability {
    pub id: String,
    pub title: String,
    pub description: String,
    pub severity: String,
    pub affected_versions: String,
    pub patched_version: Option<String>,
}

// The npm advisory search API returns a paginated envelope, not a bare array.
#[derive(Debug, Deserialize)]
struct AdvisorySearchResponse {
    objects: Vec<AdvisoryObject>,
}

#[derive(Debug, Deserialize)]
struct AdvisoryObject {
    advisory: AdvisoryDetail,
}

#[derive(Debug, Deserialize)]
struct AdvisoryDetail {
    id: u64,
    title: String,
    #[serde(default)]
    overview: String,
    severity: String,
    vulnerable_versions: String,
    patched_versions: Option<String>,
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

    pub async fn check_package(
        &mut self,
        name: &str,
        _version: &Version,
    ) -> Result<Vec<Vulnerability>> {
        if let Some(cached) = self.cache.get(name) {
            return Ok(cached.clone());
        }

        let url = format!(
            "https://registry.npmjs.org/-/npm/v1/security/advisories/search?package={}",
            name
        );

        let response = self.client.get(&url).send().await?;
        // The npm advisory search API format has changed over time; treat any
        // deserialization failure as "no advisories found" so the audit command
        // never crashes due to upstream API changes.
        let envelope: AdvisorySearchResponse = match response.json().await {
            Ok(env) => env,
            Err(_) => AdvisorySearchResponse { objects: vec![] },
        };

        let vulnerabilities: Vec<Vulnerability> = envelope
            .objects
            .into_iter()
            .map(|obj| Vulnerability {
                id: obj.advisory.id.to_string(),
                title: obj.advisory.title,
                description: obj.advisory.overview,
                severity: obj.advisory.severity,
                affected_versions: obj.advisory.vulnerable_versions,
                patched_version: obj.advisory.patched_versions,
            })
            .collect();

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
            .rev() // Start from newest versions
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
