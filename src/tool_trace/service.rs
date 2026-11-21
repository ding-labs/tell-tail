//! Explicit, reversible per-user macOS service installation. No login shell or hooks.
use super::{receiver, setup_config};
use clap::{Parser, Subcommand};
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, OpenOptions},
    io::{self, Read, Write},
    net::{Ipv4Addr, TcpListener},
    os::unix::fs::{OpenOptionsExt, PermissionsExt},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};

mod lifecycle;

const LABEL: &str = "ing.ding.tell-tail.tracing";
const MAX_CONFIG: u64 = 2 * 1024 * 1024;

#[derive(Parser)]
#[command(
    name = "tell-tail tracing",
    about = "Manage persistent local Codex tracing on macOS"
)]
pub struct Args {
    #[command(subcommand)]
    command: Action,
}
#[derive(Subcommand)]
enum Action {
    /// Install a login service and owned exporter settings; restart Codex once after first setup.
    Enable {
        #[arg(long, default_value_t = 63701)]
        trace_port: u16,
        #[arg(long, default_value_t = 61193)]
        web_port: u16,
        /// Inspect local chat turns, nested calls, and bounded command inputs.
        #[arg(long)]
        inspect_calls: bool,
        /// Migrate a previous receiver credential without putting its value on the command line.
        #[arg(long)]
        import_token_file: Option<PathBuf>,
    },
    /// Show service, exporter, and receiver health without exposing credentials.
    Status,
    /// Replace the managed collector with this executable, preserving configuration and credentials.
    Upgrade {
        /// Enable local call inspection, or pass --inspect-calls=false to turn it off.
        #[arg(long, num_args = 0..=1, require_equals = true, default_missing_value = "true")]
        inspect_calls: Option<bool>,
    },
    /// Restore the previous collector executable; keep the same credential and ports.
    Rollback,
    /// Disable capture and remove managed files, including credentials and the setup backup.
    Uninstall,
    /// Remove only Tell-Tail's exporter settings and unload its login service.
    Disable,
}

#[derive(Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum CollectorCommand {
    #[default]
    Mcp,
    Desktop,
}
impl CollectorCommand {
    fn argument(self) -> &'static str {
        match self {
            Self::Mcp => "mcp",
            Self::Desktop => "desktop",
        }
    }
}
#[derive(Clone, Serialize, Deserialize)]
struct Install {
    #[serde(default)]
    version: Option<String>,
    #[serde(default)]
    command: CollectorCommand,
    config: PathBuf,
    #[serde(default)]
    inspect_calls: bool,
    #[serde(default)]
    trace_links: bool,
    trace_port: u16,
    web_port: u16,
}
struct Paths {
    root: PathBuf,
    plist: PathBuf,
    config: PathBuf,
    domain: String,
    label: String,
}
impl Paths {
    fn discover() -> io::Result<Self> {
        let home = std::path::absolute(PathBuf::from(
            std::env::var_os("HOME").ok_or_else(|| io::Error::other("HOME unavailable"))?,
        ))?;
        let codex = std::env::var_os("CODEX_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| home.join(".codex"));
        let uid = Command::new("/usr/bin/id").arg("-u").output()?;
        let uid = std::str::from_utf8(&uid.stdout).unwrap_or("").trim();
        if uid.is_empty() || !uid.bytes().all(|b| b.is_ascii_digit()) {
            return Err(io::Error::other("Cannot determine login domain"));
        }
        Ok(Self {
            root: home.join("Library/Application Support/Tell-Tail"),
            plist: home
                .join("Library/LaunchAgents")
                .join(format!("{LABEL}.plist")),
            config: std::path::absolute(codex)?.join("config.toml"),
            domain: format!("gui/{uid}"),
            label: LABEL.into(),
        })
    }
    fn job(&self) -> String {
        format!("{}/{}", self.domain, self.label)
    }
    fn install(&self) -> io::Result<Install> {
        serde_json::from_str(&read(&self.root.join("install.json"))?)
            .map_err(|_| io::Error::other("Invalid Tell-Tail installation metadata"))
    }
}

fn read(path: &Path) -> io::Result<String> {
    if fs::symlink_metadata(path)?.file_type().is_symlink() {
        return Err(io::Error::other("Refusing a symlinked setup file"));
    }
    let mut file = crate::platform::open_regular(path)?;
    let mut value = String::new();
    (&mut file)
        .take(MAX_CONFIG + 1)
        .read_to_string(&mut value)?;
    if value.len() as u64 > MAX_CONFIG {
        return Err(io::Error::other("Setup file exceeds size limit"));
    }
    Ok(value)
}
fn read_optional(path: &Path) -> io::Result<Option<String>> {
    match read(path) {
        Ok(v) => Ok(Some(v)),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e),
    }
}
fn private_dir(path: &Path) -> io::Result<()> {
    fs::create_dir_all(path)?;
    if fs::symlink_metadata(path)?.file_type().is_symlink() {
        return Err(io::Error::other("Refusing a symlinked setup directory"));
    }
    fs::set_permissions(path, fs::Permissions::from_mode(0o700))
}
fn write_atomic(path: &Path, bytes: &[u8], expected: Option<&str>) -> io::Result<()> {
    if read_optional(path)?.as_deref() != expected {
        return Err(io::Error::other(
            "Setup file changed concurrently; no overwrite performed",
        ));
    }
    let mut tmp = tempfile::NamedTempFile::new_in(
        path.parent()
            .ok_or_else(|| io::Error::other("Missing parent"))?,
    )?;
    tmp.as_file()
        .set_permissions(fs::Permissions::from_mode(0o600))?;
    tmp.write_all(bytes)?;
    tmp.as_file().sync_all()?;
    if read_optional(path)?.as_deref() != expected {
        return Err(io::Error::other(
            "Setup file changed concurrently; no overwrite performed",
        ));
    }
    tmp.persist(path).map_err(|e| e.error)?;
    Ok(())
}
fn xml(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}
fn plist(root: &Path, install: &Install, label: &str) -> String {
    let mut arguments = vec![
        root.join("tell-tail").display().to_string(),
        install.command.argument().into(),
        "--source".into(),
        "codex-desktop".into(),
        "--output".into(),
        "web".into(),
        "--trace-port".into(),
        install.trace_port.to_string(),
        "--trace-token-file".into(),
        root.join("trace.key").display().to_string(),
        "--web-port".into(),
        install.web_port.to_string(),
    ];
    if install.inspect_calls {
        arguments.push("--inspect-chat-index".into());
        arguments.push(
            install
                .config
                .with_file_name("session_index.jsonl")
                .display()
                .to_string(),
        );
    }
    let arguments = arguments
        .iter()
        .map(|a| format!("<string>{}</string>", xml(a)))
        .collect::<String>();
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n<plist version=\"1.0\"><dict><key>Label</key><string>{label}</string><key>ProgramArguments</key><array>{arguments}</array><key>RunAtLoad</key><true/><key>KeepAlive</key><true/><key>ThrottleInterval</key><integer>10</integer><key>ExitTimeOut</key><integer>5</integer><key>ProcessType</key><string>Background</string><key>StandardOutPath</key><string>/dev/null</string><key>StandardErrorPath</key><string>/dev/null</string></dict></plist>\n"
    )
}
fn launchctl(args: &[&str]) -> io::Result<bool> {
    let mut child = Command::new("/bin/launchctl")
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;
    let until = Instant::now() + Duration::from_secs(10);
    loop {
        if let Some(status) = child.try_wait()? {
            return Ok(status.success());
        }
        if Instant::now() >= until {
            let _ = child.kill();
            let _ = child.wait();
            return Err(io::Error::other("launchctl timed out"));
        }
        thread::sleep(Duration::from_millis(50));
    }
}
fn response_json(port: u16, path: &str) -> Option<serde_json::Value> {
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(1)))
        .max_redirects(0)
        .max_response_header_size(8192)
        .build()
        .into();
    let mut response = agent
        .get(format!("http://127.0.0.1:{port}{path}"))
        .call()
        .ok()?;
    let body = response
        .body_mut()
        .with_config()
        .limit(64 * 1024)
        .read_to_string()
        .ok()?;
    serde_json::from_str(&body).ok()
}
fn snapshot(port: u16) -> Option<serde_json::Value> {
    let value = response_json(port, "/api/traces/snapshot")?;
    (value["adapter"] == "codex-otlp-tool-result-v1"
        && value["enabled"] == true
        && matches!(
            value["status"].as_str(),
            Some("waiting_for_exporter" | "receiving" | "quiet")
        ))
    .then_some(value)
}
fn healthy(install: &Install) -> bool {
    snapshot(install.web_port).is_some()
        && (install.command == CollectorCommand::Mcp
            || response_json(install.web_port, "/api/activity/snapshot")
                .is_some_and(|v| v["schema_version"] == 1))
}
fn status(paths: &Paths) -> io::Result<()> {
    let installed = paths.root.join("install.json").exists();
    let mut value = serde_json::json!({"installed":installed,"service_loaded":launchctl(&["print", &paths.job()])?});
    if installed {
        let install = paths.install()?;
        let owned = read(&paths.root.join("otel.toml"))?;
        let config = read_optional(&install.config)?.unwrap_or_default();
        value["exporter_configured"] = setup_config::is_enabled(&config, &owned).into();
        value["dashboard"] = format!("http://127.0.0.1:{}/", install.web_port).into();
        value["turn_links_configured"] =
            (install.trace_links && setup_config::is_enabled(&config, &owned)).into();
        value["trace_port"] = install.trace_port.into();
        value["installed_version"] = install.version.clone().into();
        value["collector_command"] = install.command.argument().into();
        value["activity_healthy"] =
            (install.command == CollectorCommand::Desktop && healthy(&install)).into();
        value["manager_version"] = env!("CARGO_PKG_VERSION").into();
        value["rollback_available"] = paths.root.join("tell-tail.previous").is_file().into();
        value["receiver"] = snapshot(install.web_port).unwrap_or(serde_json::Value::Null);
    }
    println!("{}", serde_json::to_string_pretty(&value)?);
    Ok(())
}

fn enable(
    paths: &Paths,
    trace_port: u16,
    web_port: u16,
    import: Option<&Path>,
    inspect_calls: bool,
    executable: &Path,
) -> io::Result<()> {
    if trace_port == 0 || web_port == 0 || trace_port == web_port {
        return Err(io::Error::other("Use two distinct, nonzero stable ports"));
    }
    if launchctl(&["print", &paths.job()])? {
        let installed = paths.install()?;
        if inspect_calls && (!installed.inspect_calls || !installed.trace_links) {
            return Err(io::Error::other(
                "Use tracing upgrade --inspect-calls to enable inspection on the running collector",
            ));
        }
        if installed.config != paths.config
            || installed.trace_port != trace_port
            || installed.web_port != web_port
            || import.is_some()
        {
            return Err(io::Error::other(
                "Service already loaded with different settings; disable it before changing installation settings",
            ));
        }
        let owned = read(&paths.root.join("otel.toml"))?;
        let config = read_optional(&paths.config)?.unwrap_or_default();
        if !setup_config::is_enabled(&config, &owned) || !healthy(&installed) {
            return Err(io::Error::other(
                "Service exists but is unhealthy or settings changed; run tracing status",
            ));
        }
        return status(paths);
    }
    // Fail before touching client settings when either port belongs to another process.
    let intake = TcpListener::bind((Ipv4Addr::LOCALHOST, trace_port))?;
    let dashboard = TcpListener::bind((Ipv4Addr::LOCALHOST, web_port))?;
    if paths.plist.exists() && !paths.root.join("install.json").exists() {
        return Err(io::Error::other(
            "An unmanaged service definition already exists; preserved it",
        ));
    }
    if paths.plist.exists()
        && read(&paths.plist)? != plist(&paths.root, &paths.install()?, &paths.label)
    {
        return Err(io::Error::other(
            "Service definition changed since setup; preserved it",
        ));
    }
    let old = read_optional(&paths.config)?;
    let owned = read_optional(&paths.root.join("otel.toml"))?;
    let key_path = paths.root.join("trace.key");
    if let Some(import) = import {
        let _ = read(import)?; // Import must already exist; never generate a migration credential.
        let key = receiver::token(import)?;
        if key_path.exists() {
            if receiver::token(&key_path)? != key {
                return Err(io::Error::other(
                    "Existing installation credential differs; preserved it",
                ));
            }
        } else {
            write_atomic(&key_path, format!("{key}\n").as_bytes(), None)?;
        }
    }
    let key = receiver::token(&key_path)?;
    let fragment = setup_config::fragment(trace_port, &key, inspect_calls);
    let updated = setup_config::enable(old.as_deref().unwrap_or(""), owned.as_deref(), &fragment)?;
    if paths.root.join("install.json").exists() && paths.install()?.config != paths.config {
        return Err(io::Error::other(
            "Installation belongs to another CODEX_HOME; use that home to disable it first",
        ));
    }
    fs::create_dir_all(paths.config.parent().unwrap())?;
    fs::create_dir_all(paths.plist.parent().unwrap())?;
    let backup = paths.root.join("config.before-setup.toml");
    if !backup.exists() {
        write_atomic(&backup, old.as_deref().unwrap_or("").as_bytes(), None)?;
    }
    lifecycle::copy_binary(executable, &paths.root.join("tell-tail"))?;
    let install = Install {
        version: Some(env!("CARGO_PKG_VERSION").into()),
        command: CollectorCommand::Desktop,
        inspect_calls,
        trace_links: inspect_calls,
        config: paths.config.clone(),
        trace_port,
        web_port,
    };
    let metadata = paths.root.join("install.json");
    write_atomic(
        &metadata,
        &serde_json::to_vec(&install)?,
        read_optional(&metadata)?.as_deref(),
    )?;
    write_atomic(
        &paths.root.join("otel.toml"),
        fragment.as_bytes(),
        owned.as_deref(),
    )?;
    write_atomic(
        &paths.plist,
        plist(&paths.root, &install, &paths.label).as_bytes(),
        read_optional(&paths.plist)?.as_deref(),
    )?;
    drop((intake, dashboard));
    let activate = || -> io::Result<()> {
        if !launchctl(&[
            "bootstrap",
            &paths.domain,
            paths
                .plist
                .to_str()
                .ok_or_else(|| io::Error::other("Non-UTF8 service path"))?,
        ])? {
            return Err(io::Error::other(
                "Could not load login service; client settings unchanged",
            ));
        }
        let until = Instant::now() + Duration::from_secs(10);
        while !healthy(&install) {
            if Instant::now() >= until {
                return Err(io::Error::other(
                    "Collector did not become healthy; client settings unchanged",
                ));
            }
            thread::sleep(Duration::from_millis(100));
        }
        write_atomic(&paths.config, updated.as_bytes(), old.as_deref())
    };
    if let Err(error) = activate() {
        let _ = launchctl(&["bootout", &paths.job()]);
        let _ = fs::remove_file(&paths.plist);
        return Err(error);
    }
    status(paths)?;
    eprintln!(
        "Tracing installed. Restart Codex once if this endpoint/credential was not already loaded. Capture remains enabled across app restarts and starts at login. History is in memory."
    );
    Ok(())
}
fn disable(paths: &Paths) -> io::Result<()> {
    if !paths.root.join("install.json").exists() {
        return Err(io::Error::other("No managed tracing installation found"));
    }
    let install = paths.install()?;
    if paths.plist.exists() && read(&paths.plist)? != plist(&paths.root, &install, &paths.label) {
        return Err(io::Error::other(
            "Service definition changed since setup; preserved it",
        ));
    }
    let owned = read(&paths.root.join("otel.toml"))?;
    let old = read_optional(&install.config)?;
    let updated = setup_config::disable(old.as_deref().unwrap_or(""), &owned)?;
    if old.is_some() {
        write_atomic(&install.config, updated.as_bytes(), old.as_deref())?;
    }
    if launchctl(&["print", &paths.job()])? && !launchctl(&["bootout", &paths.job()])? {
        return Err(io::Error::other(
            "Exporter settings removed, but service unload failed; retry tracing disable",
        ));
    }
    if paths.plist.exists() {
        fs::remove_file(&paths.plist)?;
    }
    // bootout can return before the process has released its listeners.
    // Wait for bounded shutdown so an immediate enable is reliable.
    let until = Instant::now() + Duration::from_secs(10);
    loop {
        if let (Ok(_intake), Ok(_dashboard)) = (
            TcpListener::bind((Ipv4Addr::LOCALHOST, install.trace_port)),
            TcpListener::bind((Ipv4Addr::LOCALHOST, install.web_port)),
        ) {
            break;
        }
        if Instant::now() >= until {
            return Err(io::Error::other(
                "Tracing disabled, but ports are still occupied; wait for shutdown before re-enabling",
            ));
        }
        thread::sleep(Duration::from_millis(100));
    }
    println!(
        "Tracing disabled. Restart Codex to unload its in-memory exporter. Private backup and credential retained for re-enabling."
    );
    Ok(())
}

pub fn run(args: Args) -> io::Result<()> {
    if !cfg!(target_os = "macos") {
        return Err(io::Error::other(
            "Managed tracing currently supports macOS; use manual exporter setup on other platforms",
        ));
    }
    let paths = Paths::discover()?;
    if matches!(args.command, Action::Status) {
        return status(&paths);
    }
    if matches!(args.command, Action::Uninstall) && !paths.root.join("install.json").exists() {
        if launchctl(&["print", &paths.job()])? || paths.plist.exists() {
            return Err(io::Error::other(
                "Service exists without installation metadata; preserved it for manual recovery",
            ));
        }
        if [
            "trace.key",
            "otel.toml",
            "tell-tail",
            "tell-tail.previous",
            "install.previous.json",
            "config.before-setup.toml",
        ]
        .iter()
        .any(|name| paths.root.join(name).exists())
        {
            return Err(io::Error::other(
                "Managed files exist without installation metadata; preserved them for manual recovery",
            ));
        }
        println!("Tell-Tail collector is already uninstalled.");
        return Ok(());
    }
    let uninstalling = matches!(args.command, Action::Uninstall);
    let executable = std::env::current_exe()?;
    private_dir(&paths.root)?;
    let lock_path = paths.root.join("setup.lock");
    let _lock = OpenOptions::new().write(true).create_new(true).mode(0o600).open(&lock_path)
        .map_err(|_| io::Error::other("Setup is already running or was interrupted; inspect/remove setup.lock before retrying"))?;
    let result = match args.command {
        Action::Enable {
            trace_port,
            web_port,
            import_token_file,
            inspect_calls,
        } => enable(
            &paths,
            trace_port,
            web_port,
            import_token_file.as_deref(),
            inspect_calls,
            &executable,
        ),
        Action::Disable => disable(&paths),
        Action::Upgrade { inspect_calls } => lifecycle::upgrade(&paths, &executable, inspect_calls),
        Action::Rollback => lifecycle::rollback(&paths),
        Action::Uninstall => lifecycle::uninstall(&paths),
        Action::Status => unreachable!(),
    };
    drop(_lock);
    let _ = fs::remove_file(lock_path);
    if uninstalling && result.is_ok() {
        // Remove only an empty managed directory; preserve any unrecognized user files.
        let _ = fs::remove_dir(&paths.root);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn atomic_writes_are_private_and_refuse_concurrent_edits() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config");
        write_atomic(&path, b"first", None).unwrap();
        assert_eq!(
            fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
        fs::write(&path, "concurrent").unwrap();
        assert!(write_atomic(&path, b"replacement", Some("first")).is_err());
        assert_eq!(read(&path).unwrap(), "concurrent");
    }
    #[test]
    fn symlinks_are_not_followed() {
        let dir = tempfile::tempdir().unwrap();
        let original = dir.path().join("original");
        fs::write(&original, "preserve").unwrap();
        let link = dir.path().join("link");
        std::os::unix::fs::symlink(&original, &link).unwrap();
        assert!(write_atomic(&link, b"replace", Some("preserve")).is_err());
        assert_eq!(fs::read_to_string(original).unwrap(), "preserve");
    }
    #[test]
    fn service_has_no_shell_or_credential_arguments_and_escapes_paths() {
        let data = plist(
            Path::new("/Users/test/A & B"),
            &Install {
                version: None,
                command: CollectorCommand::Desktop,
                config: PathBuf::new(),
                inspect_calls: false,
                trace_links: false,
                trace_port: 63701,
                web_port: 61193,
            },
            LABEL,
        );
        assert!(data.contains("A &amp; B/tell-tail"));
        assert!(data.contains("<key>KeepAlive</key><true/>"));
        assert!(!data.contains("/bin/sh"));
        assert!(!data.contains("x-tell-tail-probe-key"));
        assert!(Args::try_parse_from(["tracing", "enable"]).is_ok());
    }
}
