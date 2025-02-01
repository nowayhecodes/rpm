use anyhow::Result;
use rpm::{
    cli::Cli,
    package::PackageJson,
    security::SecurityChecker,
};
use std::path::PathBuf;
use tempfile::tempdir;
use tokio;

async fn setup_test_environment() -> Result<tempfile::TempDir> {
    let temp_dir = tempdir()?;
    std::env::set_current_dir(&temp_dir)?;
    Ok(temp_dir)
}

#[tokio::test]
async fn test_package_installation() -> Result<()> {
    let _temp_dir = setup_test_environment().await?;

    // Create a minimal package.json
    let package_json = PackageJson {
        name: "test-project".to_string(),
        version: "1.0.0".to_string(),
        dependencies: Some([("express".to_string(), "^4.17.1".to_string())].into()),
        dev_dependencies: None,
    };
    package_json.save().await?;

    // Test installation
    let cli = Cli::parse_from(&["rpm", "install", "express"]);
    cli.execute().await?;

    // Verify installation
    assert!(PathBuf::from("node_modules/express").exists());
    assert!(PathBuf::from("node_modules/express/package.json").exists());

    Ok(())
}

#[tokio::test]
async fn test_security_audit() -> Result<()> {
    let _temp_dir = setup_test_environment().await?;

    // Create a package.json with a known vulnerable package
    let package_json = PackageJson {
        name: "test-project".to_string(),
        version: "1.0.0".to_string(),
        dependencies: Some([("lodash".to_string(), "4.17.15".to_string())].into()),
        dev_dependencies: None,
    };
    package_json.save().await?;

    // Test audit
    let cli = Cli::parse_from(&["rpm", "audit"]);
    cli.execute().await?;

    // Test audit --fix
    let cli = Cli::parse_from(&["rpm", "audit", "--fix"]);
    cli.execute().await?;

    // Verify fix
    let updated_package_json = PackageJson::load().await?;
    let deps = updated_package_json.dependencies.unwrap();
    assert_ne!(deps.get("lodash").unwrap(), "4.17.15");

    Ok(())
}

#[tokio::test]
async fn test_parallel_installation() -> Result<()> {
    let _temp_dir = setup_test_environment().await?;

    // Create package.json with multiple dependencies
    let package_json = PackageJson {
        name: "test-project".to_string(),
        version: "1.0.0".to_string(),
        dependencies: Some([
            ("express".to_string(), "^4.17.1".to_string()),
            ("lodash".to_string(), "^4.17.21".to_string()),
            ("react".to_string(), "^17.0.2".to_string()),
        ].into()),
        dev_dependencies: None,
    };
    package_json.save().await?;

    // Test parallel installation
    let cli = Cli::parse_from(&["rpm", "install", "express", "lodash", "react"]);
    cli.execute().await?;

    // Verify all packages are installed
    assert!(PathBuf::from("node_modules/express").exists());
    assert!(PathBuf::from("node_modules/lodash").exists());
    assert!(PathBuf::from("node_modules/react").exists());

    Ok(())
}

#[tokio::test]
async fn test_update_command() -> Result<()> {
    let _temp_dir = setup_test_environment().await?;

    // Create package.json with an old version
    let package_json = PackageJson {
        name: "test-project".to_string(),
        version: "1.0.0".to_string(),
        dependencies: Some([
            ("lodash".to_string(), "4.17.15".to_string()),
        ].into()),
        dev_dependencies: None,
    };
    package_json.save().await?;

    // Test update
    let cli = Cli::parse_from(&["rpm", "update"]);
    cli.execute().await?;

    // Verify update
    let updated_package_json = PackageJson::load().await?;
    let deps = updated_package_json.dependencies.unwrap();
    assert_ne!(deps.get("lodash").unwrap(), "4.17.15");

    Ok(())
}

#[tokio::test]
async fn test_remove_command() -> Result<()> {
    let _temp_dir = setup_test_environment().await?;

    // Create package.json and install a package
    let package_json = PackageJson {
        name: "test-project".to_string(),
        version: "1.0.0".to_string(),
        dependencies: Some([
            ("express".to_string(), "^4.17.1".to_string()),
        ].into()),
        dev_dependencies: None,
    };
    package_json.save().await?;

    let cli = Cli::parse_from(&["rpm", "install", "express"]);
    cli.execute().await?;

    // Test remove
    let cli = Cli::parse_from(&["rpm", "remove", "express"]);
    cli.execute().await?;

    // Verify removal
    assert!(!PathBuf::from("node_modules/express").exists());
    let updated_package_json = PackageJson::load().await?;
    assert!(updated_package_json.dependencies.unwrap().is_empty());

    Ok(())
} 