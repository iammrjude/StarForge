# Pull Request: Feature Batch — Issues #240, #245, #248, #251

## Summary

- **Deploy dry-run mode** (`--dry-run`) validates the full deployment path without submitting any transaction, printing an actionable deployment plan with fee estimates and warnings.
- **Template metadata fields** adds `license`, `repository`, `homepage`, and `documentation` URL fields to template entries, exposed via CLI publish flags and shown in `template show`.
- **Plugin update command** adds `starforge plugin update [name]` to upgrade installed plugins from their registered sources without manual reinstalls.
- **Template publish auditing** requires README.md, validates semver strings for `version` and version constraints, and checks that `cli_version_min <= cli_version_max` — all with actionable error messages.

## Changes

### `src/commands/deploy.rs` — closes #240
- Added `--dry-run` flag to `DeployArgs`
- Added `run_dry_run()` function that runs 4 sequential checks:
  1. WASM artifact path + magic-byte validation
  2. Wallet existence in local config
  3. Network connectivity and account XLM balance via Horizon
  4. Soroban fee estimation via RPC simulation
- Prints a deployment plan summary with warnings and the exact `stellar contract deploy` command to run

### `src/utils/templates.rs` + `src/commands/template.rs` — closes #251, #248
- Added `license`, `repository`, `homepage`, `documentation` optional fields to `TemplateEntry` (serde-defaulted for backward compatibility)
- Exposed all four as `--license`, `--repository`, `--homepage`, `--documentation` flags on `starforge template publish`
- Displayed in `template show` and `template publish` output
- Added `validate_template_structure_with_constraints()` — the full audit entry-point used by `publish_template_versioned`:
  - Requires `README.md` (actionable error)
  - Validates `version`, `cli_version_min`, `cli_version_max` as valid semver
  - Rejects `cli_version_min > cli_version_max`
  - Error messages name the exact field and explain the fix
- `make_valid_template()` test helper now creates `README.md`
- New tests: missing README, bad version semver, bad constraint semver, min > max

### `src/plugins/registry.rs` + `src/commands/plugin.rs` — closes #245
- Added `version: Option<String>` and `installed_at: Option<String>` to `InstalledPlugin` (serde-defaulted)
- `install_plugin()` now records `installed_at` timestamp (ISO-8601 UTC)
- Added `Update { name: Option<String>, yes: bool }` variant to `PluginCommands`
- `update()` function:
  - For crates.io sources: runs `cargo install --force`
  - For other trusted sources: compares library mtime to `installed_at` and refreshes registry if newer
  - Local-path plugins: reported as unupdatable with guidance
  - Unknown sources: blocked unless `--yes` is passed
  - Reports updated / skipped / failed counts

## Test Plan

- [x] `cargo build` succeeds without warnings
- [x] `cargo test` — all tests pass (no regressions)
- [ ] `starforge deploy --wasm <file> --dry-run` — prints plan, no transaction submitted
- [ ] `starforge template publish <path> --name t --description d --author a --license MIT --repository https://github.com/org/repo` — stores and displays metadata
- [ ] `starforge template show <name>` — displays license/repository/homepage/docs
- [ ] `starforge template publish <path> --name t --description d --author a` with no README.md — fails with actionable error
- [ ] `starforge plugin update` — checks all plugins and reports status
- [ ] `starforge plugin update <name>` — updates single named plugin

closes #240
closes #245
closes #248
closes #251

🤖 Generated with [Claude Code](https://claude.com/claude-code)
