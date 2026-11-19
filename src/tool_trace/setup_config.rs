//! Own only the exporter table we installed; preserve unrelated user settings.
use std::io;
use toml_edit::DocumentMut;

fn parse(raw: &str) -> io::Result<DocumentMut> {
    raw.parse()
        .map_err(|_| io::Error::other("Invalid TOML configuration (contents withheld)"))
}

fn same_table(a: &DocumentMut, b: &DocumentMut) -> bool {
    a.get("otel").map(ToString::to_string) == b.get("otel").map(ToString::to_string)
}

pub fn is_enabled(raw: &str, owned: &str) -> bool {
    matches!((parse(raw), parse(owned)), (Ok(a), Ok(b)) if a.contains_key("otel") && same_table(&a, &b))
}

pub fn enable(raw: &str, previously_owned: Option<&str>, fragment: &str) -> io::Result<String> {
    let mut config = parse(raw)?;
    let new = parse(fragment)?;
    if config.contains_key("otel") {
        let owned = previously_owned.map(parse).transpose()?;
        if !owned.as_ref().is_some_and(|o| same_table(&config, o)) {
            return Err(io::Error::other(
                "Existing or edited [otel] settings preserved; automatic setup cannot replace another exporter",
            ));
        }
    }
    config["otel"] = new["otel"].clone();
    Ok(config.to_string())
}

pub fn disable(raw: &str, owned: &str) -> io::Result<String> {
    let mut config = parse(raw)?;
    if config.contains_key("otel") && !same_table(&config, &parse(owned)?) {
        return Err(io::Error::other(
            "The [otel] settings changed since setup; preserved them. Restore or remove your exporter manually before stopping capture",
        ));
    }
    config.remove("otel");
    Ok(config.to_string())
}

pub fn fragment(port: u16, token: &str) -> String {
    // Token is validated by receiver::token before use; no user text interpolation.
    format!(
        "[otel]\nlog_user_prompt = false\nlog_agent_responses = false\nexporter = {{ otlp-http = {{ endpoint = \"http://127.0.0.1:{port}/v1/logs\", protocol = \"binary\", headers = {{ \"x-tell-tail-probe-key\" = \"{token}\" }} }} }}\n"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ownership_survives_round_trip_and_preserves_unrelated_edits() {
        let original =
            "# user preferences\nmodel = 'my-model'\n\n[plugins.example]\nenabled = true\n";
        let fragment = fragment(63701, &"a".repeat(64));
        let installed = enable(original, None, &fragment).unwrap();
        assert_eq!(
            enable(&installed, Some(&fragment), &fragment).unwrap(),
            installed
        );
        let changed = installed.replace("enabled = true", "enabled = false");
        assert_eq!(
            disable(&changed, &fragment).unwrap(),
            original.replace("enabled = true", "enabled = false")
        );
    }
    #[test]
    fn refuses_existing_exporters_and_edited_owned_settings_without_leaking_values() {
        let fragment = fragment(63701, &"a".repeat(64));
        for raw in [
            "[otel]\nexporter = 'PRIVATE'",
            "otel = { exporter = 'PRIVATE' }",
        ] {
            let error = enable(raw, None, &fragment).unwrap_err().to_string();
            assert!(!error.contains("PRIVATE"));
            assert!(disable(raw, &fragment).is_err());
        }
        let edited = fragment.replace("63701", "63702");
        assert!(enable(&edited, Some(&fragment), &fragment).is_err());
        assert!(disable(&edited, &fragment).is_err());
        assert!(enable("PRIVATE = [", None, &fragment).is_err());
    }
    #[test]
    fn missing_table_can_be_reenabled_or_disabled() {
        let fragment = fragment(63701, &"a".repeat(64));
        assert_eq!(
            disable("model = 'x'\n", &fragment).unwrap(),
            "model = 'x'\n"
        );
        assert!(
            enable("", Some(&fragment), &fragment)
                .unwrap()
                .contains("[otel]")
        );
    }
}
