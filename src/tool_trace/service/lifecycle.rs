//! Collector replacement is independent of plugin installation and exporter configuration.
use super::*;

pub(super) fn copy_binary(source: &Path, destination: &Path) -> io::Result<()> {
    let mut input = crate::platform::open_regular(source)?;
    let mut temporary = tempfile::NamedTempFile::new_in(destination.parent().unwrap())?;
    io::copy(&mut input, &mut temporary)?;
    temporary
        .as_file()
        .set_permissions(fs::Permissions::from_mode(0o700))?;
    temporary.as_file().sync_all()?;
    temporary
        .persist(destination)
        .map_err(|error| error.error)?;
    Ok(())
}

fn stop(paths: &Paths, install: &Install) -> io::Result<()> {
    if launchctl(&["print", &paths.job()])? && !launchctl(&["bootout", &paths.job()])? {
        return Err(io::Error::other("Could not stop the managed collector"));
    }
    let until = Instant::now() + Duration::from_secs(10);
    loop {
        if let (Ok(_intake), Ok(_dashboard)) = (
            TcpListener::bind((Ipv4Addr::LOCALHOST, install.trace_port)),
            TcpListener::bind((Ipv4Addr::LOCALHOST, install.web_port)),
        ) {
            return Ok(());
        }
        if Instant::now() >= until {
            return Err(io::Error::other("Collector ports were not released"));
        }
        thread::sleep(Duration::from_millis(100));
    }
}

fn start(paths: &Paths, install: &Install) -> io::Result<()> {
    let plist = paths
        .plist
        .to_str()
        .ok_or_else(|| io::Error::other("Non-UTF8 service path"))?;
    if !launchctl(&["bootstrap", &paths.domain, plist])? {
        return Err(io::Error::other("Could not load the managed collector"));
    }
    let until = Instant::now() + Duration::from_secs(10);
    loop {
        if healthy(install) {
            return Ok(());
        }
        if Instant::now() >= until {
            return Err(io::Error::other(
                "Replacement collector did not become healthy",
            ));
        }
        thread::sleep(Duration::from_millis(100));
    }
}

fn replace(
    paths: &Paths,
    source: &Path,
    version: Option<String>,
    command: CollectorCommand,
    inspecting: Option<bool>,
) -> io::Result<()> {
    let mut install = paths.install()?;
    let metadata_path = paths.root.join("install.json");
    let original_metadata = read(&metadata_path)?;
    let original_config = read_optional(&install.config)?;
    let owned = read(&paths.root.join("otel.toml"))?;
    // Validate ownership even for a disabled installation; never reconfigure a different exporter.
    setup_config::disable(original_config.as_deref().unwrap_or(""), &owned)?;
    let configured = setup_config::is_enabled(original_config.as_deref().unwrap_or(""), &owned);
    let loaded = launchctl(&["print", &paths.job()])?;
    if loaded && !configured {
        return Err(io::Error::other(
            "Collector is loaded without its owned exporter; resolve setup before upgrading",
        ));
    }
    let original_plist = read_optional(&paths.plist)?;
    if (configured && original_plist.is_none())
        || original_plist
            .as_ref()
            .is_some_and(|v| *v != plist(&paths.root, &install, &paths.label))
    {
        return Err(io::Error::other(
            "Service definition changed since setup; preserved it",
        ));
    }
    // Stage before replacing the backup so rollback can use the previous binary as its input.
    let candidate = tempfile::NamedTempFile::new_in(&paths.root)?;
    copy_binary(source, candidate.path())?;
    let current = paths.root.join("tell-tail");
    let previous = paths.root.join("tell-tail.previous");
    let previous_metadata = paths.root.join("install.previous.json");
    copy_binary(&current, &previous)?;
    write_atomic(
        &previous_metadata,
        original_metadata.as_bytes(),
        read_optional(&previous_metadata)?.as_deref(),
    )?;
    let original_install = install.clone();
    install.version = version;
    install.command = command;
    if let Some(enable) = inspecting {
        install.inspect_calls = enable;
    }
    let replacement_plist = plist(&paths.root, &install, &paths.label);
    let transaction = || -> io::Result<()> {
        if configured {
            stop(paths, &original_install)?;
        }
        copy_binary(candidate.path(), &current)?;
        if configured {
            write_atomic(
                &paths.plist,
                replacement_plist.as_bytes(),
                original_plist.as_deref(),
            )?;
            start(paths, &install)?;
        }
        // Configuration is not rewritten by upgrades. Detect edits made during activation.
        if read_optional(&install.config)? != original_config {
            return Err(io::Error::other("Client settings changed during upgrade"));
        }
        write_atomic(
            &metadata_path,
            &serde_json::to_vec(&install)?,
            Some(&original_metadata),
        )
    };
    if let Err(cause) = transaction() {
        let recover = || -> io::Result<()> {
            if configured {
                stop(paths, &original_install)?;
            }
            copy_binary(&previous, &current)?;
            if configured {
                let present = read_optional(&paths.plist)?;
                if present.as_deref() != Some(&replacement_plist) && present != original_plist {
                    return Err(io::Error::other(
                        "Service definition changed during recovery; preserved it",
                    ));
                }
                write_atomic(
                    &paths.plist,
                    original_plist.as_deref().unwrap().as_bytes(),
                    present.as_deref(),
                )?;
                start(paths, &original_install)?;
            }
            Ok(())
        };
        return match recover() {
            Ok(()) => Err(io::Error::other(format!(
                "Upgrade failed; previous collector restored: {cause}"
            ))),
            Err(_) => Err(io::Error::other(format!(
                "Upgrade failed and automatic recovery failed; previous executable and metadata remain in the private installation directory. Retry tracing rollback after resolving the service issue: {cause}"
            ))),
        };
    }
    status(paths)?;
    eprintln!(
        "Collector replaced. Credential, ports and client settings preserved. In-memory call history resets on collector restart. Use tracing rollback to restore the previous executable and launch command."
    );
    Ok(())
}

pub(super) fn upgrade(paths: &Paths, source: &Path, inspecting: Option<bool>) -> io::Result<()> {
    replace(
        paths,
        source,
        Some(env!("CARGO_PKG_VERSION").into()),
        CollectorCommand::Desktop,
        inspecting,
    )
}

pub(super) fn rollback(paths: &Paths) -> io::Result<()> {
    let old: Install = serde_json::from_str(&read(&paths.root.join("install.previous.json"))?)
        .map_err(|_| io::Error::other("Invalid previous installation metadata"))?;
    let current = paths.install()?;
    if old.config != current.config
        || old.trace_port != current.trace_port
        || old.web_port != current.web_port
    {
        return Err(io::Error::other(
            "Previous executable belongs to different installation settings; preserved it",
        ));
    }
    replace(
        paths,
        &paths.root.join("tell-tail.previous"),
        old.version,
        old.command,
        Some(old.inspect_calls),
    )
}

pub(super) fn uninstall(paths: &Paths) -> io::Result<()> {
    if paths.root.join("otel.toml").exists() {
        disable(paths)?;
    } else {
        // Resume an interrupted final cleanup only when nothing remains to disable.
        let install = paths.install()?;
        if setup_config::has_otel(&read_optional(&install.config)?.unwrap_or_default())?
            || launchctl(&["print", &paths.job()])?
            || paths.plist.exists()
        {
            return Err(io::Error::other(
                "Exporter ownership file is missing; preserve the installation until configuration and service ownership are resolved",
            ));
        }
    }
    // An explicit allowlist avoids deleting files the user added to this directory.
    // Keep install.json until last so an interrupted removal can be retried.
    for name in [
        "tell-tail",
        "tell-tail.previous",
        "install.previous.json",
        "config.before-setup.toml",
        "trace.key",
        "otel.toml",
    ] {
        let path = paths.root.join(name);
        match fs::remove_file(&path) {
            Ok(()) => (),
            Err(error) if error.kind() == io::ErrorKind::NotFound => (),
            Err(error) => return Err(error),
        }
    }
    // The marker is last: an interrupted cleanup can be retried even after otel.toml is gone.
    fs::remove_file(paths.root.join("install.json"))?;
    println!(
        "Tell-Tail collector uninstalled. Restart Codex to unload its in-memory exporter, then remove the plugin in Desktop. Unrecognized files are preserved."
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn failed_candidate_staging_preserves_installed_bytes() {
        let dir = tempfile::tempdir().unwrap();
        let current = dir.path().join("tell-tail");
        fs::write(&current, b"original").unwrap();
        assert!(copy_binary(&dir.path().join("missing"), &current).is_err());
        assert_eq!(fs::read(&current).unwrap(), b"original");
        copy_binary(&current, &dir.path().join("previous")).unwrap();
        assert_eq!(
            fs::metadata(dir.path().join("previous"))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o700
        );
    }

    #[cfg(target_os = "macos")]
    #[test]
    #[ignore = "requires a built TELL_TAIL_TEST_BINARY and a macOS GUI launchd domain"]
    fn native_lifecycle_preserves_credentials_recovers_and_uninstalls() {
        let binary = PathBuf::from(
            std::env::var_os("TELL_TAIL_TEST_BINARY").expect("Set TELL_TAIL_TEST_BINARY"),
        );
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("managed");
        private_dir(&root).unwrap();
        let label = format!(
            "ing.ding.tell-tail.test.{}",
            dir.path()
                .file_name()
                .unwrap()
                .to_string_lossy()
                .replace('.', "")
        );
        let uid = Command::new("/usr/bin/id").arg("-u").output().unwrap();
        let paths = Paths {
            root,
            plist: dir.path().join("agent.plist"),
            config: dir.path().join("config.toml"),
            domain: format!("gui/{}", String::from_utf8(uid.stdout).unwrap().trim()),
            label,
        };
        struct Cleanup<'a>(&'a Paths);
        impl Drop for Cleanup<'_> {
            fn drop(&mut self) {
                let _ = launchctl(&["bootout", &self.0.job()]);
            }
        }
        let _cleanup = Cleanup(&paths);
        let first = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        let second = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        let (intake, web) = (
            first.local_addr().unwrap().port(),
            second.local_addr().unwrap().port(),
        );
        drop((first, second));
        fs::write(&paths.config, "# preserve\nmodel = 'test'\n").unwrap();
        enable(&paths, intake, web, None, false, &binary).unwrap();
        assert!(healthy(&paths.install().unwrap()));
        // Simulate legacy metadata (without command) and its owned MCP launch definition.
        let mut legacy = paths.install().unwrap();
        stop(&paths, &legacy).unwrap();
        legacy.command = CollectorCommand::Mcp;
        let mut metadata = serde_json::to_value(&legacy).unwrap();
        metadata.as_object_mut().unwrap().remove("command");
        fs::write(
            paths.root.join("install.json"),
            serde_json::to_vec(&metadata).unwrap(),
        )
        .unwrap();
        fs::write(&paths.plist, plist(&paths.root, &legacy, &paths.label)).unwrap();
        start(&paths, &legacy).unwrap();
        assert!(response_json(web, "/api/activity/snapshot").is_none());
        let key = fs::read(paths.root.join("trace.key")).unwrap();
        let config = fs::read(&paths.config).unwrap();
        let first_epoch = snapshot(web).unwrap()["snapshot"]["collector_epoch"].clone();
        upgrade(&paths, &binary, None).unwrap();
        assert_ne!(
            snapshot(web).unwrap()["snapshot"]["collector_epoch"],
            first_epoch
        );
        assert_eq!(fs::read(paths.root.join("trace.key")).unwrap(), key);
        assert_eq!(fs::read(&paths.config).unwrap(), config);
        assert!(paths.install().unwrap().command == CollectorCommand::Desktop);
        assert!(response_json(web, "/api/activity/snapshot").is_some());
        rollback(&paths).unwrap();
        assert!(paths.install().unwrap().command == CollectorCommand::Mcp);
        assert!(response_json(web, "/api/activity/snapshot").is_none());
        let current = fs::read(paths.root.join("tell-tail")).unwrap();
        assert!(
            upgrade(&paths, Path::new("/usr/bin/false"), None)
                .unwrap_err()
                .to_string()
                .contains("previous collector restored")
        );
        assert_eq!(fs::read(paths.root.join("tell-tail")).unwrap(), current);
        assert!(snapshot(web).is_some());
        assert_eq!(fs::read(paths.root.join("trace.key")).unwrap(), key);
        assert_eq!(fs::read(&paths.config).unwrap(), config);
        let owned_plist = read(&paths.plist).unwrap();
        fs::write(&paths.plist, format!("{owned_plist}<!-- edited -->")).unwrap();
        assert!(upgrade(&paths, &binary, None).is_err());
        assert!(disable(&paths).is_err());
        assert_eq!(fs::read(&paths.config).unwrap(), config);
        fs::write(&paths.plist, owned_plist).unwrap();
        let edited = String::from_utf8(config.clone())
            .unwrap()
            .replace("log_user_prompt = false", "log_user_prompt = true");
        fs::write(&paths.config, &edited).unwrap();
        assert!(uninstall(&paths).is_err());
        assert!(paths.root.join("tell-tail").exists());
        assert!(snapshot(web).is_some());
        fs::write(&paths.config, config).unwrap();
        disable(&paths).unwrap();
        upgrade(&paths, &binary, None).unwrap();
        assert!(!launchctl(&["print", &paths.job()]).unwrap());
        assert!(!setup_config::has_otel(&read(&paths.config).unwrap()).unwrap());
        assert_eq!(fs::read(paths.root.join("trace.key")).unwrap(), key);
        enable(&paths, intake, web, None, false, &binary).unwrap();
        fs::write(paths.root.join("user-file"), "keep").unwrap();
        uninstall(&paths).unwrap();
        assert!(!launchctl(&["print", &paths.job()]).unwrap());
        assert!(!paths.plist.exists());
        assert!(!paths.root.join("trace.key").exists());
        assert!(!paths.root.join("tell-tail").exists());
        assert_eq!(read(&paths.config).unwrap(), "# preserve\nmodel = 'test'\n");
        assert_eq!(read(&paths.root.join("user-file")).unwrap(), "keep");
        // Simulate interruption after deleting the ownership fragment, before the marker.
        enable(&paths, intake, web, None, false, &binary).unwrap();
        disable(&paths).unwrap();
        fs::remove_file(paths.root.join("otel.toml")).unwrap();
        uninstall(&paths).unwrap();
        assert!(!paths.root.join("install.json").exists());
        assert_eq!(read(&paths.root.join("user-file")).unwrap(), "keep");
    }
}
