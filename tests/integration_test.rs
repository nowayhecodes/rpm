use anyhow::Result;
use rpm::{
    cache::{CacheConfig, PackageCache},
    cli::Cli,
    config::Config,
    install::PackageInstaller,
    lockfile::LockFile,
    package::PackageJson,
    profiling::MemoryProfile,
    AppContext,
};
use std::sync::Arc;
use tempfile::tempdir;
use tokio;
use tokio::sync::Mutex;

async fn setup_test_environment() -> Result<(tempfile::TempDir, AppContext)> {
    let temp_dir = tempdir()?;
    let cache_dir = temp_dir.path().join(".rpm-cache");
    let package_cache = PackageCache::new(CacheConfig {
        cache_dir,
        ..CacheConfig::default()
    })
    .await?;
    let context = AppContext::new(
        MemoryProfile::new(1024 * 1024 * 1024),
        package_cache,
        temp_dir.path().to_path_buf(),
        Config::default(),
    );
    Ok((temp_dir, context))
}

#[tokio::test]
async fn test_init_command_creates_package_json() -> Result<()> {
    let (temp_dir, context) = setup_test_environment().await?;

    let cli = Cli::parse_from(&["rpm", "init"]);
    cli.execute_with_context(context).await?;

    let package_json = PackageJson::load_from(temp_dir.path().join("package.json")).await?;
    assert_eq!(
        package_json.name,
        temp_dir
            .path()
            .file_name()
            .unwrap()
            .to_str()
            .unwrap()
            .to_ascii_lowercase()
            .trim_matches(|character| matches!(character, '-' | '_' | '.'))
            .to_string()
    );
    assert_eq!(package_json.version, "1.0.0");

    Ok(())
}

#[tokio::test]
async fn test_init_command_with_name_creates_project_directory() -> Result<()> {
    let (temp_dir, context) = setup_test_environment().await?;

    let cli = Cli::parse_from(&["rpm", "init", "--name", "my-app"]);
    cli.execute_with_context(context).await?;

    let package_json_path = temp_dir.path().join("my-app").join("package.json");
    let package_json = PackageJson::load_from(package_json_path).await?;
    assert_eq!(package_json.name, "my-app");

    Ok(())
}

#[tokio::test]
async fn test_init_command_with_typescript_creates_ts_project() -> Result<()> {
    let (temp_dir, context) = setup_test_environment().await?;

    let cli = Cli::parse_from(&["rpm", "init", "--ts", "--name", "ts-app"]);
    cli.execute_with_context(context).await?;

    let project_dir = temp_dir.path().join("ts-app");
    let package_json = PackageJson::load_from(project_dir.join("package.json")).await?;
    assert_eq!(package_json.name, "ts-app");
    assert_eq!(
        package_json
            .dev_dependencies
            .unwrap()
            .get("typescript")
            .unwrap(),
        "^6.0.3"
    );
    assert!(project_dir.join("tsconfig.json").exists());
    assert!(project_dir.join("src").join("index.ts").exists());

    Ok(())
}

#[tokio::test]
async fn test_local_install_creates_node_modules_in_project_directory() -> Result<()> {
    let (temp_dir, context) = setup_test_environment().await?;

    let lock_file = Arc::new(Mutex::new(LockFile::new(
        "test".to_string(),
        "0.0.0".to_string(),
    )));
    let installer = PackageInstaller::new_in_project(
        false,
        context.package_cache,
        context.memory_profile,
        context.project_dir,
        &context.config,
        lock_file,
    );
    installer.install_packages(&[]).await?;

    assert!(temp_dir.path().join("node_modules").exists());

    Ok(())
}

#[tokio::test]
async fn test_package_installation() -> Result<()> {
    let (temp_dir, context) = setup_test_environment().await?;

    // Create a minimal package.json
    let package_json = PackageJson {
        name: "test-project".to_string(),
        version: "1.0.0".to_string(),
        description: None,
        main: None,
        types: None,
        scripts: None,
        dependencies: Some([("express".to_string(), "^4.17.1".to_string())].into()),
        dev_dependencies: None,
        license: None,
    };
    package_json
        .save_to(temp_dir.path().join("package.json"))
        .await?;

    // Test installation
    let cli = Cli::parse_from(&["rpm", "install", "express"]);
    cli.execute_with_context(context).await?;

    // Verify installation
    assert!(temp_dir.path().join("node_modules/express").exists());
    assert!(temp_dir
        .path()
        .join("node_modules/express/package.json")
        .exists());

    Ok(())
}

#[tokio::test]
async fn test_security_audit() -> Result<()> {
    let (temp_dir, context) = setup_test_environment().await?;

    // Create a package.json with a known vulnerable package
    let package_json = PackageJson {
        name: "test-project".to_string(),
        version: "1.0.0".to_string(),
        description: None,
        main: None,
        types: None,
        scripts: None,
        dependencies: Some([("lodash".to_string(), "4.17.15".to_string())].into()),
        dev_dependencies: None,
        license: None,
    };
    package_json
        .save_to(temp_dir.path().join("package.json"))
        .await?;

    // Test audit
    let cli = Cli::parse_from(&["rpm", "audit"]);
    cli.execute_with_context(context.clone()).await?;

    // Test audit --fix
    let cli = Cli::parse_from(&["rpm", "audit", "--fix"]);
    cli.execute_with_context(context).await?;

    // Verify fix
    let updated_package_json = PackageJson::load_from(temp_dir.path().join("package.json")).await?;
    let deps = updated_package_json.dependencies.unwrap();
    assert_ne!(deps.get("lodash").unwrap(), "4.17.15");

    Ok(())
}

#[tokio::test]
async fn test_parallel_installation() -> Result<()> {
    let (temp_dir, context) = setup_test_environment().await?;

    // Create package.json with multiple dependencies
    let package_json = PackageJson {
        name: "test-project".to_string(),
        version: "1.0.0".to_string(),
        description: None,
        main: None,
        types: None,
        scripts: None,
        dependencies: Some(
            [
                ("express".to_string(), "^4.17.1".to_string()),
                ("lodash".to_string(), "^4.17.21".to_string()),
                ("react".to_string(), "^17.0.2".to_string()),
            ]
            .into(),
        ),
        dev_dependencies: None,
        license: None,
    };
    package_json
        .save_to(temp_dir.path().join("package.json"))
        .await?;

    // Test parallel installation
    let cli = Cli::parse_from(&["rpm", "install", "express", "lodash", "react"]);
    cli.execute_with_context(context).await?;

    // Verify all packages are installed
    assert!(temp_dir.path().join("node_modules/express").exists());
    assert!(temp_dir.path().join("node_modules/lodash").exists());
    assert!(temp_dir.path().join("node_modules/react").exists());

    Ok(())
}

#[tokio::test]
async fn test_update_command() -> Result<()> {
    let (temp_dir, context) = setup_test_environment().await?;

    // Create package.json with an old version
    let package_json = PackageJson {
        name: "test-project".to_string(),
        version: "1.0.0".to_string(),
        description: None,
        main: None,
        types: None,
        scripts: None,
        dependencies: Some([("lodash".to_string(), "4.17.15".to_string())].into()),
        dev_dependencies: None,
        license: None,
    };
    package_json
        .save_to(temp_dir.path().join("package.json"))
        .await?;

    // Test update
    let cli = Cli::parse_from(&["rpm", "update"]);
    cli.execute_with_context(context).await?;

    // Verify update
    let updated_package_json = PackageJson::load_from(temp_dir.path().join("package.json")).await?;
    let deps = updated_package_json.dependencies.unwrap();
    assert_ne!(deps.get("lodash").unwrap(), "4.17.15");

    Ok(())
}

#[tokio::test]
async fn test_remove_command() -> Result<()> {
    let (temp_dir, context) = setup_test_environment().await?;

    // Create package.json and install a package
    let package_json = PackageJson {
        name: "test-project".to_string(),
        version: "1.0.0".to_string(),
        description: None,
        main: None,
        types: None,
        scripts: None,
        dependencies: Some([("express".to_string(), "^4.17.1".to_string())].into()),
        dev_dependencies: None,
        license: None,
    };
    package_json
        .save_to(temp_dir.path().join("package.json"))
        .await?;

    let cli = Cli::parse_from(&["rpm", "install", "express"]);
    cli.execute_with_context(context.clone()).await?;

    // Test remove
    let cli = Cli::parse_from(&["rpm", "remove", "express"]);
    cli.execute_with_context(context).await?;

    // Verify removal
    assert!(!temp_dir.path().join("node_modules/express").exists());
    let updated_package_json = PackageJson::load_from(temp_dir.path().join("package.json")).await?;
    assert!(updated_package_json.dependencies.unwrap().is_empty());

    Ok(())
}
