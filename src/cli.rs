use crate::{
    cache::{CacheConfig, PackageCache},
    dependency::DependencyResolver,
    error::RpmResult,
    install::PackageInstaller,
    package::PackageJson,
    profiling::MemoryProfile,
    registry::RegistryClient,
    security::SecurityChecker,
    AppContext,
};
use clap::{Parser, Subcommand};
use log::{debug, info};
use semver::Version;
use std::{
    collections::{BTreeMap, HashMap},
    ffi::OsString,
    path::{Path, PathBuf},
    sync::Arc,
};
use tokio::fs;

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
pub struct Cli {
    #[command(subcommand)]
    command: Commands,

    #[arg(short, long, global = true)]
    pub verbose: bool,
}

#[derive(Subcommand)]
pub enum Commands {
    Init {
        #[arg(long)]
        ts: bool,
        #[arg(long)]
        name: Option<String>,
    },
    Install {
        packages: Vec<String>,
        #[arg(short, long)]
        global: bool,
    },
    Update {
        packages: Vec<String>,
    },
    Remove {
        packages: Vec<String>,
        #[arg(short, long)]
        global: bool,
    },
    List {
        #[arg(short, long)]
        global: bool,
    },
    Audit {
        #[arg(long)]
        fix: bool,
    },
}

impl Cli {
    pub fn parse_from<I, T>(itr: I) -> Self
    where
        I: IntoIterator<Item = T>,
        T: Into<OsString> + Clone,
    {
        <Self as Parser>::parse_from(itr)
    }

    pub async fn execute(self) -> RpmResult<()> {
        let memory_profile = MemoryProfile::new(1024 * 1024 * 1024);
        let package_cache = PackageCache::new(CacheConfig::default()).await?;
        let project_dir = std::env::current_dir()?;
        self.execute_with_context(AppContext {
            memory_profile,
            package_cache,
            project_dir,
        })
        .await
    }

    pub async fn execute_with_context(self, context: AppContext) -> RpmResult<()> {
        match self.command {
            Commands::Init { ts, name } => {
                Self::init_project(&context, name, ts).await?;
            }
            Commands::Install { packages, global } => {
                let packages = if packages.is_empty() {
                    let package_json = PackageJson::load_from(context.package_json_path()).await?;
                    let mut all_packages = Vec::new();
                    if let Some(deps) = package_json.dependencies {
                        all_packages.extend(deps.into_keys());
                    }
                    if let Some(dev_deps) = package_json.dev_dependencies {
                        all_packages.extend(dev_deps.into_keys());
                    }
                    all_packages
                } else {
                    packages
                };
                debug!("Installing packages: {:?}", packages);
                let installer = PackageInstaller::new_in_project(
                    global,
                    context.package_cache,
                    context.memory_profile,
                    context.project_dir,
                );
                installer.install_packages(&packages).await?;
                info!("Successfully installed packages: {:?}", packages);
            }
            Commands::Update { packages } => {
                let package_json = PackageJson::load_from(context.package_json_path()).await?;
                let registry = Arc::new(RegistryClient::new());
                let resolver = DependencyResolver::new(registry);

                // Strip leading range operators (^, ~, >=, etc.) to get a bare version
                // for comparison. package.json stores ranges, not exact versions.
                let bare_version = |s: &str| -> Option<Version> {
                    let stripped = s.trim_start_matches(|c: char| {
                        matches!(c, '^' | '~' | '>' | '<' | '=' | ' ')
                    });
                    Version::parse(stripped).ok()
                };

                if !packages.is_empty() {
                    for package in packages {
                        if let Some(deps) = &package_json.dependencies {
                            if let Some(current_version) = deps.get(&package) {
                                println!("Updating {} from version {}", package, current_version);

                                let latest = resolver
                                    .resolve_single_dependency(
                                        &package,
                                        &semver::VersionReq::parse("*").unwrap(),
                                    )
                                    .await?;

                                let should_update = bare_version(current_version)
                                    .map_or(true, |cv| latest.version > cv);

                                if should_update {
                                    let package_name = package.clone();
                                    let installer = PackageInstaller::new_in_project(
                                        false,
                                        context.package_cache.clone(),
                                        context.memory_profile.clone(),
                                        context.project_dir.clone(),
                                    );
                                    installer.install_packages(&[package_name]).await?;
                                    println!("Updated {} to version {}", package, latest.version);
                                } else {
                                    println!("{} is already at the latest version", package);
                                }
                            }
                        }
                    }
                } else {
                    if let Some(deps) = &package_json.dependencies {
                        for (package, current_version) in deps {
                            println!(
                                "Checking updates for {} (current: {})",
                                package, current_version
                            );

                            let latest = resolver
                                .resolve_single_dependency(
                                    package,
                                    &semver::VersionReq::parse("*").unwrap(),
                                )
                                .await?;

                            let should_update = bare_version(current_version)
                                .map_or(true, |cv| latest.version > cv);

                            if should_update {
                                let package_name = package.clone();
                                let installer = PackageInstaller::new_in_project(
                                    false,
                                    context.package_cache.clone(),
                                    context.memory_profile.clone(),
                                    context.project_dir.clone(),
                                );
                                installer.install_packages(&[package_name]).await?;
                                println!("Updated {} to version {}", package, latest.version);
                            } else {
                                println!("{} is already at the latest version", package);
                            }
                        }
                    }
                }
            }
            Commands::Remove { packages, global } => {
                let base_path = if global {
                    PathBuf::from("/usr/local/lib/node_modules")
                } else {
                    context.node_modules_path()
                };

                for package in packages {
                    let package_path = base_path.join(&package);
                    if package_path.exists() {
                        fs::remove_dir_all(&package_path).await?;
                        println!("Successfully removed package: {}", package);

                        if !global {
                            let package_json_path = context.package_json_path();
                            if let Ok(mut package_json) =
                                PackageJson::load_from(&package_json_path).await
                            {
                                package_json.remove_dependency(&package);
                                package_json.save_to(&package_json_path).await?;
                            }
                        }
                    } else {
                        println!("Package not found: {}", package);
                    }
                }
            }
            Commands::List { global } => {
                println!("Installed packages:");

                // Read package.json for local packages
                if let Ok(package_json) = PackageJson::load_from(context.package_json_path()).await
                {
                    println!("\nLocal packages:");
                    if let Some(deps) = &package_json.dependencies {
                        for (name, version) in deps {
                            println!("  {} @ {}", name, version);
                        }
                    }

                    if let Some(dev_deps) = &package_json.dev_dependencies {
                        println!("\nDev dependencies:");
                        for (name, version) in dev_deps {
                            println!("  {} @ {}", name, version);
                        }
                    }
                }

                // List global packages
                let global_dir = PathBuf::from("/usr/local/lib/node_modules");
                if global && global_dir.exists() {
                    println!("\nGlobal packages:");
                    let mut entries = fs::read_dir(global_dir).await?;
                    while let Some(entry) = entries.next_entry().await? {
                        if entry.file_type().await?.is_dir() {
                            if let Some(name) = entry.file_name().to_str() {
                                if let Ok(package_json) =
                                    PackageJson::load_from(entry.path().join("package.json")).await
                                {
                                    println!("  {} @ {}", name, package_json.version);
                                }
                            }
                        }
                    }
                }
            }
            Commands::Audit { fix } => {
                println!("Auditing packages for security vulnerabilities...");

                let package_json_path = context.package_json_path();
                let mut package_json = PackageJson::load_from(&package_json_path).await?;
                let mut security_checker = SecurityChecker::new();
                let mut vulnerabilities_found = false;
                let mut fixes_applied = false;

                if let Some(deps) = &mut package_json.dependencies {
                    let mut updates = Vec::new();

                    for (name, version_str) in deps.iter() {
                        let version = Version::parse(version_str)?;
                        let vulns = security_checker.check_package(name, &version).await?;

                        if !vulns.is_empty() {
                            vulnerabilities_found = true;
                            println!("\nVulnerabilities found in {}", name);

                            for vuln in &vulns {
                                println!("\nID: {}", vuln.id);
                                println!("Title: {}", vuln.title);
                                println!("Severity: {}", vuln.severity);
                                println!("Description: {}", vuln.description);

                                if let Some(patched) = &vuln.patched_version {
                                    println!("Patched version: {}", patched);
                                }
                            }

                            if fix {
                                // Get all available versions from registry
                                let registry = Arc::new(RegistryClient::new());
                                let package_info = registry.fetch_package_info(name, None).await?;
                                let available_versions = vec![package_info.version]; // Simplified for now

                                if let Ok(safe_version) = security_checker
                                    .find_safe_version(name, &version, &available_versions)
                                    .await
                                {
                                    updates.push((name.clone(), safe_version.to_string()));
                                    fixes_applied = true;
                                }
                            }
                        }
                    }

                    // Apply fixes if requested
                    if fix && fixes_applied {
                        for (name, new_version) in updates {
                            deps.insert(name.clone(), new_version.clone());
                            println!("Updated {} to version {}", name, new_version);
                        }
                        package_json.save_to(&package_json_path).await?;
                        println!("\nUpdated package.json with security fixes");
                    }
                }

                if !vulnerabilities_found {
                    println!("No vulnerabilities found!");
                } else if !fix {
                    println!("\nRun 'rpm audit --fix' to automatically fix these issues");
                }
            }
        }

        Ok(())
    }

    async fn init_project(context: &AppContext, name: Option<String>, ts: bool) -> RpmResult<()> {
        let (project_dir, package_name) = match name {
            Some(name) => {
                let sanitized = sanitize_package_name(&name);
                (context.project_dir.join(&sanitized), sanitized)
            }
            None => {
                let name = context
                    .project_dir
                    .file_name()
                    .and_then(|name| name.to_str())
                    .map(sanitize_package_name)
                    .unwrap_or_else(|| "rpm-project".to_string());
                (context.project_dir.clone(), name)
            }
        };

        fs::create_dir_all(&project_dir).await?;

        let package_json_path = project_dir.join("package.json");
        if package_json_path.exists() {
            return Err(anyhow::anyhow!(
                "package.json already exists at {}",
                package_json_path.display()
            )
            .into());
        }

        let package_json = if ts {
            typescript_package_json(package_name)
        } else {
            PackageJson::new(package_name)
        };
        package_json.save_to(&package_json_path).await?;

        if ts {
            write_typescript_project_files(&project_dir).await?;
        }

        println!("Initialized project in {}", project_dir.display());
        Ok(())
    }
}

fn sanitize_package_name(name: &str) -> String {
    let sanitized = name
        .trim()
        .to_ascii_lowercase()
        .chars()
        .map(|character| match character {
            'a'..='z' | '0'..='9' | '-' | '_' | '.' => character,
            _ => '-',
        })
        .collect::<String>()
        .trim_matches(|character| matches!(character, '-' | '_' | '.'))
        .to_string();

    if sanitized.is_empty() {
        "rpm-project".to_string()
    } else {
        sanitized
    }
}

fn typescript_package_json(name: String) -> PackageJson {
    PackageJson {
        name,
        version: "1.0.0".to_string(),
        description: None,
        main: Some("dist/index.js".to_string()),
        types: Some("dist/index.d.ts".to_string()),
        scripts: Some(BTreeMap::from([
            ("build".to_string(), "tsc".to_string()),
            ("start".to_string(), "node dist/index.js".to_string()),
            ("typecheck".to_string(), "tsc --noEmit".to_string()),
        ])),
        dependencies: None,
        dev_dependencies: Some(HashMap::from([(
            "typescript".to_string(),
            "^6.0.3".to_string(),
        )])),
        license: Some("ISC".to_string()),
    }
}

async fn write_typescript_project_files(project_dir: &Path) -> RpmResult<()> {
    fs::create_dir_all(project_dir.join("src")).await?;

    let tsconfig = serde_json::json!({
        "compilerOptions": {
            "target": "ES2022",
            "module": "NodeNext",
            "moduleResolution": "NodeNext",
            "rootDir": "src",
            "outDir": "dist",
            "declaration": true,
            "sourceMap": true,
            "strict": true,
            "esModuleInterop": true,
            "forceConsistentCasingInFileNames": true,
            "skipLibCheck": true
        },
        "include": ["src/**/*.ts"],
        "exclude": ["node_modules", "dist"]
    });

    fs::write(
        project_dir.join("tsconfig.json"),
        serde_json::to_string_pretty(&tsconfig)?,
    )
    .await?;
    fs::write(
        project_dir.join("src").join("index.ts"),
        "console.log(\"Hello from rpm + TypeScript\");\n",
    )
    .await?;

    Ok(())
}
