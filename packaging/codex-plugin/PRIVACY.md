# Tell-Tail desktop plugin privacy policy

This policy covers the Tell-Tail desktop plugin from Ding Labs. The bundled collector runs on your Mac. Installing the plugin puts the executable and setup instructions on your machine. Collection starts when you open a local session or enable persistent capture.

## What the collector reads

- **Desktop activity.** Tell-Tail reads supported records from the Codex desktop logs and, if present, Claude Desktop MCP logs. It uses them to show connection health, errors, counts, and event times. Its activity history contains selected diagnostic fields such as server names and event types.
- **Tool-call tracing.** If you enable persistent capture, Codex sends telemetry to an authenticated receiver on your Mac. The dashboard shows available tool and provider names, timing, outcomes, and temporary identifiers that group related results. Incoming records can contain tool inputs or outputs. The receiver discards those fields before storing observations. The setup turns off prompt logging in the exporter it configures.
- **Application logs you add.** Tell-Tail reads the files you select to calculate metrics and show recent errors. A preview can contain up to 512 bytes of original log text, including personal information or secrets written by the application. The path and name you assign to a source are visible in your local dashboard.

## Who can see the data

The collector serves the dashboard on your Mac's loopback interface. It does not send logs, traces, or usage analytics to Ding Labs. No Tell-Tail account is required. Other processes on the Mac that can reach the dashboard can read its displayed data.

When you ask ChatGPT or Codex to inspect Tell-Tail, the results it reads for you can become part of that conversation and are handled under that service's data settings. Copying a diagnostic summary, sharing a screenshot, or posting a support request also shares the information you choose to include with its recipients.

## Retention and removal

The collector keeps a limited history of observations, metrics, and previews in memory. Older entries expire as the buffers fill, and stopping the collector clears its memory. A later session can read records still present in the original logs.

Persistent setup installs a per-user login service, writes exporter settings to Codex, and keeps a local receiver credential and a private backup of the pre-setup Codex configuration. The managed collector also saves the names, paths, and settings of application log sources you add in `~/Library/Application Support/Tell-Tail/log-sources.json`.

- To stop persistent capture and keep your setup for later, ask Tell-Tail to **disable tracing**, then restart Codex.
- To remove the service, its owned exporter settings, credential, and setup backup, ask Tell-Tail to **uninstall the collector** before removing the plugin in the desktop app. Restart Codex to unload the exporter.
- To forget a saved log source, remove it in **Logs → Manage sources**. After uninstall, you can delete the saved `log-sources.json` file if it remains. Your original application and desktop logs stay under your control.

For privacy questions, contact Ding Labs through [Tell-Tail support](https://github.com/ding-labs/tell-tail/issues). Issues are public. Describe the question without attaching logs, credentials, or configuration backups.
