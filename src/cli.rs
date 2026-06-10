use crate::{
    cache::{CacheConfig, PackageCache},
    config::Config,
    dependency::DependencyResolver,
    error::RpmResult,
    install::PackageInstaller,
    lockfile::LockFile,
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
    path::Path,
    sync::Arc,
};
use tokio::{fs, sync::Mutex};

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
            config: Config::default(),
        })
        .await
    }

    pub async fn execute_with_context(self, context: AppContext) -> RpmResult<()> {
        match self.command {
            Commands::Init { ts, name } => {
                Self::init_project(&context, name, ts).await?;
            }

            Commands::Install { packages, global } => {
                let is_explicit = !packages.is_empty();

                // Load root package.json for the lock-file header; fall back to a
                // placeholder if the project has not been initialized yet.
                let root_pkg =
                    PackageJson::load_from(context.package_json_path())
                        .await
                        .unwrap_or_else(|_| PackageJson::new("project"));

                let lock_file = Arc::new(Mutex::new(LockFile::new(
                    root_pkg.name.clone(),
                    root_pkg.version.clone(),
                )));

                // When called with no explicit packages, check for an existing lock
                // file and replay it (exact pinned versions, no BFS resolution).
                if !is_explicit {
                    let lock_path = context.project_dir.join("rpm-lock.json");
                    if lock_path.exists() {
                        let existing = LockFile::load(&lock_path).await?;
                        let locked: Vec<(String, String)> = existing
                            .dependencies
                            .into_iter()
                            .map(|(name, dep)| (name, dep.version))
                            .collect();

                        let installer = PackageInstaller::new_in_project(
                            global,
                            context.package_cache.clone(),
                            context.memory_profile.clone(),
                            &context.project_dir,
                            &context.config,
                            Arc::clone(&lock_file),
                        );
                        installer.install_locked_packages(&locked).await?;
                        info!("Installed {} packages from rpm-lock.json", locked.len());
                        return Ok(());
                    }
                }

                // Resolve the package list: explicit args, or everything in
                // package.json dependencies + devDependencies.
                let packages_to_install: Vec<String> = if is_explicit {
                    packages.clone()
                } else {
                    let mut all = Vec::new();
                    if let Some(deps) = &root_pkg.dependencies {
                        all.extend(deps.keys().cloned());
                    }
                    if let Some(dev_deps) = &root_pkg.dev_dependencies {
                        all.extend(dev_deps.keys().cloned());
                    }
                    all
                };

                debug!("Installing packages: {:?}", packages_to_install);

                let installer = PackageInstaller::new_in_project(
                    global,
                    context.package_cache.clone(),
                    context.memory_profile.clone(),
                    &context.project_dir,
                    &context.config,
                    Arc::clone(&lock_file),
                );
                installer.install_packages(&packages_to_install).await?;
                info!("Successfully installed packages: {:?}", packages_to_install);

                // When explicit packages were requested (not a bare `rpm install`),
                // write them into package.json so the manifest stays in sync.
                if is_explicit && !global {
                    let lock = lock_file.lock().await;
                    let pkg_json_path = context.package_json_path();
                    let mut pkg_json =
                        PackageJson::load_from(&pkg_json_path)
                            .await
                            .unwrap_or_else(|_| PackageJson::new(root_pkg.name.clone()));

                    let deps = pkg_json.dependencies.get_or_insert_with(HashMap::new);
                    for pkg_name in &packages {
                        if let Some(locked_dep) = lock.get_dependency(pkg_name) {
                            deps.insert(
                                pkg_name.clone(),
                                format!("^{}", locked_dep.version),
                            );
                        }
                    }
                    drop(lock);

                    pkg_json.save_to(&pkg_json_path).await?;
                    info!("Updated package.json with installed packages");
                }
            }

            Commands::Update { packages } => {
                let mut package_json =
                    PackageJson::load_from(context.package_json_path()).await?;
                let registry = Arc::new(RegistryClient::with_config(&context.config));
                let resolver = DependencyResolver::new(Arc::clone(&registry));

                let lock_file = Arc::new(Mutex::new(LockFile::new(
                    package_json.name.clone(),
                    package_json.version.clone(),
                )));

                let bare_version = |s: &str| -> Option<Version> {
                    let stripped = s.trim_start_matches(|c: char| {
                        matches!(c, '^' | '~' | '>' | '<' | '=' | ' ')
                    });
                    Version::parse(stripped).ok()
                };

                let deps_snapshot: Vec<(String, String)> = package_json
                    .dependencies
                    .as_ref()
                    .map(|d| d.iter().map(|(k, v)| (k.clone(), v.clone())).collect())
                    .unwrap_or_default();

                let mut version_updates: Vec<(String, String)> = Vec::new();

                let targets: Vec<(String, String)> = if !packages.is_empty() {
                    packages
                        .iter()
                        .filter_map(|p| {
                            deps_snapshot.iter().find(|(name, _)| name == p).cloned()
                        })
                        .collect()
                } else {
                    deps_snapshot.clone()
                };

                for (package, current_version) in &targets {
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
                        let installer = PackageInstaller::new_in_project(
                            false,
                            context.package_cache.clone(),
                            context.memory_profile.clone(),
                            context.project_dir.clone(),
                            &context.config,
                            Arc::clone(&lock_file),
                        );
                        installer.install_packages(&[package.clone()]).await?;
                        println!("Updated {} to version {}", package, latest.version);
                        version_updates.push((package.clone(), latest.version.to_string()));
                    } else {
                        println!("{} is already at the latest version", package);
                    }
                }

                if !version_updates.is_empty() {
                    if let Some(deps) = &mut package_json.dependencies {
                        for (name, new_version) in &version_updates {
                            deps.insert(name.clone(), new_version.clone());
                        }
                    }
                    package_json.save_to(context.package_json_path()).await?;
                }
            }

            Commands::Remove { packages, global } => {
                let base_path = if global {
                    context.config.global_packages_dir.clone()
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

                if let Ok(package_json) =
                    PackageJson::load_from(context.package_json_path()).await
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

                let global_dir = &context.config.global_packages_dir;
                if global && global_dir.exists() {
                    println!("\nGlobal packages:");
                    let mut entries = fs::read_dir(global_dir).await?;
                    while let Some(entry) = entries.next_entry().await? {
                        if entry.file_type().await?.is_dir() {
                            if let Some(name) = entry.file_name().to_str() {
                                if let Ok(pkg_json) =
                                    PackageJson::load_from(entry.path().join("package.json"))
                                        .await
                                {
                                    println!("  {} @ {}", name, pkg_json.version);
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

                let deps_snapshot: Vec<(String, String)> = package_json
                    .dependencies
                    .as_ref()
                    .map(|d| d.iter().map(|(k, v)| (k.clone(), v.clone())).collect())
                    .unwrap_or_default();

                let mut updates: Vec<(String, String)> = Vec::new();

                for (name, version_str) in &deps_snapshot {
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
                    }

                    if fix {
                        let registry = Arc::new(RegistryClient::with_config(&context.config));
                        let package_info = registry.fetch_package_info(name, None).await?;
                        let latest_version = package_info.version.to_string();
                        if &latest_version != version_str {
                            updates.push((name.clone(), latest_version));
                        }
                    }
                }

                if fix && !updates.is_empty() {
                    if let Some(deps) = &mut package_json.dependencies {
                        for (name, new_version) in &updates {
                            deps.insert(name.clone(), new_version.clone());
                            println!("Updated {} to version {}", name, new_version);
                        }
                    }
                    package_json.save_to(&package_json_path).await?;
                    println!("\nUpdated package.json with security fixes");
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
