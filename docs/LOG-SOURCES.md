# Application log sources in Tell-Tail

Starting with Tell-Tail 0.5.0, the desktop/plugin dashboard can follow
user-selected application files alongside AI Activity, without another dashboard,
process, or plugin installation. Existing file/stdin CLI commands remain unchanged.

## Add and inspect a file

1. Open **Logs → Manage sources**, or **AI Activity → Sources → Application logs**.
2. Choose **Add log file**, give it a name, and paste its absolute local path.
   On macOS, Finder's Option → Copy as Pathname supplies this path. This is a
   local file follower, not a browser upload or remote file picker.
3. Choose **New entries only** (the default) or **Beginning of file**. Missing
   files remain in a waiting state until they appear. Only regular files are
   supported; directories, devices, sockets and named pipes are rejected.
4. Set a format or expand **Parsing and limits** when needed, then save.
5. Select the file in Logs, or use **View logs** from Sources.

Each file uses the original engine: byte/record accounting, rotation and truncation
handling, throughput and severity history, parsing coverage, exact values,
recent error/fatal previews, ingestion diagnostics, and display freezing. Profiles
include Pino, Python, OpenTelemetry and Serilog; Docker/CRI envelopes, custom fields
and nested JSON severity paths are available. Invalid combinations leave the form
and existing source unchanged. Terminal, stdin, Prometheus and webhook capabilities
remain available through their existing CLI commands; this is not a new alert editor.

The desktop aggregate remains a separate choice in Logs. Its metadata-only metrics
and counting semantics are unchanged. Application log bodies do not enter the AI
Activity store, filters, timeline, or tool-call tracing pipeline. Selecting different
sources reloads the metrics view so histories and counters cannot mix. The current
selection follows the Logs link back from AI Activity within the same browser tab.

## Control monitoring

- **Freeze display** preserves the visible metrics while collection continues.
- **Pause** in source management stops that file's follower. Its final snapshot
  stays available in the current collector session; a collector restart clears it.
- **Resume** or **Edit → Save** starts a new metrics session. Editing only the name
  also starts a new session; the form discloses this before saving.
- The chosen read position applies on every start, resume, or collector restart.
  **New entries only** skips existing data at that time; **Beginning of file**
  rereads the file. There are no persisted offsets or exactly-once ingestion claims.
- **Remove** asks for confirmation, stops the follower, and removes its saved
  configuration. It never deletes, truncates, or changes the original log file.

Open views refresh when a source is changed from another tab. Removed sources or
paused sources without retained metrics show an explicit unavailable state rather
than continuing to display another source's metrics. Source-list polling leaves
unchanged keyboard controls in place.

## Saved settings and limits

Managed plugin collectors automatically use `log-sources.json` beside their trace
credential, normally in the private Tell-Tail application-support directory. Enabled
sources restart automatically; paused sources stay paused. Source configuration is
stored locally with private file permissions and atomic replacement. Invalid
registries fail explicitly and are preserved. A lock prevents two collectors from
writing the same registry concurrently. A failed save leaves active sources unchanged.

Standalone `desktop` or `mcp` web sessions keep sources only for that process unless
a registry is selected explicitly:

```sh
tell-tail desktop --open --sources-file "$HOME/.config/tell-tail/log-sources.json"
```

The UI identifies whether settings persist. Demo/custom test collectors have no
implicit shared registry. Managed upgrades retain the registry; current uninstall
also leaves saved source settings in place, alongside other preserved user data.
Delete the registry explicitly after stopping/uninstalling the collector to forget
those settings. User log files are always left untouched.

At most **16** application sources can be configured, including paused sources.
Each has the original bounded parser, 60 completed seconds of chart history, and
at most 20 recent error/fatal previews capped at 512 bytes each. The record limit
defaults to 1 MiB per source (maximum 16 MiB); the rolling window defaults to 60
seconds (1–3600). Source settings are capped at 128 KiB and mutation bodies at 8 KiB.
Per-file streams share the desktop aggregate's four-stream connection budget.

## Local API and privacy

Read-only list and metrics endpoints:

- `GET /api/log-sources`
- `GET /api/log-sources/{id}/snapshot`
- `GET /api/log-sources/{id}/events`

Management endpoints:

- `POST /api/log-sources` — create a configured follower.
- `PUT /api/log-sources/{id}` — replace its configuration.
- `POST /api/log-sources/{id}` — `{"action":"pause"}` or `{"action":"resume"}`.
- `DELETE /api/log-sources/{id}` — remove the follower and saved configuration.

The listener stays on loopback. Host and Origin checks protect all endpoints;
mutations additionally require the exact dashboard Origin and
`X-Tell-Tail-Action: manage-sources`, with JSON bodies for POST/PUT. There is no CORS
opt-in, directory listing, arbitrary byte-download endpoint, or shell execution.
The API grants the local dashboard authority to follow explicitly supplied paths;
this is broader than the original read-only desktop API. Existing APIs remain
read-only and retain their original contracts.

Names and paths are visible locally in Sources and saved settings. Unlike automatic
AI capture, manually added logs intentionally expose original error previews in
Logs. The add form discloses this distinction. Previews and metrics remain in memory;
only source settings persist. No beta packaging or release is part of this feature.

See the [validation record](VALIDATION-LOG-SOURCES.md) for browser, lifecycle,
visual and installed-collector checks.
