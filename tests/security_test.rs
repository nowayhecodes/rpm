use rpm::security::{SecurityChecker, Vulnerability};
use anyhow::Result;
use mockall::automock;
use semver::Version;

#[automock]
trait SecurityCheck {
    async fn check_package(&mut self, name: &str, version: &Version) -> Result<Vec<Vulnerability>>;
    async fn find_safe_version(
        &mut self,
        name: &str,
        current_version: &Version,
        available_versions: &[Version],
    ) -> Result<Version>;
}

#[tokio::test]
async fn test_security_checker_with_mocks() -> Result<()> {
    let mut mock = MockSecurityCheck::new();
    
    // Setup expectations
    mock.expect_check_package()
        .returning(|_, _| {
            Ok(vec![Vulnerability {
                id: "CVE-2021-1234".to_string(),
                title: "Test vulnerability".to_string(),
                description: "Test description".to_string(),
                severity: "high".to_string(),
                affected_versions: "<=4.17.15".to_string(),
                patched_version: Some("4.17.16".to_string()),
            }])
        });

    // Test vulnerability check
    let version = Version::parse("4.17.15")?;
    let vulns = mock.check_package("test-package", &version).await?;
    
    assert_eq!(vulns.len(), 1);
    assert_eq!(vulns[0].id, "CVE-2021-1234");
    assert_eq!(vulns[0].severity, "high");

    Ok(())
} 