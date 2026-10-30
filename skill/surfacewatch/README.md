# SurfaceWatch skill

A skill for AI agents that use **SurfaceWatch**, the desktop app that captures the client-side attack surface of web apps in an authorised pentest engagement and compares captures between builds. The
skill tells the agent how to use SurfaceWatch's tools well: which to call first, what
needs your yes, sensible defaults, and how to report back.

## What you need

- **SurfaceWatch**, installed from <https://mayorana.ch/en/apps/surfacewatch> (macOS,
  Windows or Linux). This skill does not include the app.
- A desktop agent connected to SurfaceWatch's MCP server: Claude Desktop, Claude
  Code, Codex CLI or Cursor. `references/connect.md` has the steps. Web-only
  agents such as ChatGPT can't reach an app on your computer.

## What you can ask for

- Summarise what changed in a target's surface since the baseline
- Read a capture's flags: server errors, admin or debug paths, object IDs, missing headers
- List findings, their REA evidence and retests
- Point out which findings a new build may affect
- Check that REA, Node.js and the browser are ready

## What waits for your yes

- Nothing: these tools only read. Captures, retests and edits stay in the SurfaceWatch window or surfacewatch-cli

## Limits

For engagements you are authorised to test only. Flags are leads for a tester, not verdicts.

## Install

- **Claude Code**: unzip into `~/.claude/skills/` so the folder is
  `~/.claude/skills/surfacewatch/`.
- **claude.ai or Claude Desktop**: upload the zip under Skills in Settings.
- **Other agents**: put the `surfacewatch` folder where the agent reads skills.

## Help

Questions, licences and bug reports: <https://mayorana.ch/en/contact>.
