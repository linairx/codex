set working-directory := "codex-rs"
set positional-arguments := true

export JUST_SHELL := justfile_directory() / "scripts/just-shell.py"

set shell := ["python3", "-c", 'import os, runpy; runpy.run_path(os.environ["JUST_SHELL"], run_name="__main__")']
set windows-shell := ["python", "-c", 'import os, runpy; runpy.run_path(os.environ["JUST_SHELL"], run_name="__main__")']

rust_min_stack := "8388608"
python := if os_family() == "windows" { "python" } else { "python3" }

# Display help
help:
    just -l

# `codex`

alias c := codex

codex *args:
    cargo run --bin codex -- {args}

# `codex exec`
exec *args:
    cargo run --bin codex -- exec {args}

# Start `codex exec-server` and run codex-tui.
[no-cd]
[positional-arguments]
[unix]
tui-with-exec-server *args:
    {{ justfile_directory() }}/scripts/run_tui_with_exec_server.sh "$@"

# Run the CLI version of the file-search crate.
file-search *args:
    cargo run --bin codex-file-search -- {args}

# Run the standalone code-mode host from source.
code-mode-host *args:
    cargo run --bin codex-code-mode-host -- {args}

# Build the CLI and run the app-server test client
app-server-test-client *args:
    cargo build -p codex-cli
    cargo run -p codex-app-server-test-client -- --codex-bin ./target/debug/codex {args}

# Format the justfile, Rust, Bazel/Starlark, Python SDK code, and Python scripts.
fmt:
    @{{ python }} ../scripts/format.py

# Check formatting without modifying files.
fmt-check:
    @{{ python }} ../scripts/format.py --check

fix *args:
    cargo clippy --fix --tests --allow-dirty {args}

clippy *args:
    cargo clippy --tests {args}

[unix]
install:
    rustup show active-toolchain
    cargo fetch

[windows]
install:
    #!powershell.exe -File
    $pwsh = Get-Command pwsh.exe -ErrorAction SilentlyContinue
    if (-not $pwsh) {
        winget install --exact --id Microsoft.PowerShell --source winget --accept-package-agreements --accept-source-agreements
        if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
    }
    rustup show active-toolchain
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
    cargo fetch
    exit $LASTEXITCODE

# Run nextest with --no-fail-fast so all tests are run.
#
# Run `cargo install --locked cargo-nextest` if you don't have it installed.
# Prefer this for routine local runs. Workspace crate features are banned, so

# there should be no need to add `--all-features`.
[unix]
test *args:
    RUST_MIN_STACK={{ rust_min_stack }} NEXTEST_PROFILE=local cargo nextest run --no-fail-fast "$@"

[windows]
test *args:
    $env:RUST_MIN_STACK = "{{ rust_min_stack }}"; $env:NEXTEST_PROFILE = "local"; cargo nextest run --no-fail-fast @($args | Select-Object -Skip 1)

# Run from the repository root so scripts that resolve paths from `cwd` see

# the same layout they use in GitHub Actions.
[no-cd]
test-github-scripts:
    {{ python }} -m unittest discover -s {{ justfile_directory() }}/.github/scripts -p 'test_*.py'

# Run explicit workspace benchmark targets.
bench *args:
    cargo bench --workspace --bench '*' {args}

# Run benchmark targets once to ensure they start successfully.
bench-smoke:
    just bench -- --test

# Run Bazel-backed end-to-end macrobenchmarks with optimized binaries.
bench-e2e:
    # Keep measured binaries comparable to production-style optimized builds.
    bazel test --compilation_mode=opt --cache_test_results=no --test_output=streamed //codex-rs:e2e-benchmarks

# Run Bazel-backed end-to-end macrobenchmarks once per case with release-like
# Rust cfg paths but fastbuild codegen.
bench-e2e-smoke:
    # Avoid optimizer cost because smoke runs only check that benchmarks work.
    # Compile target Rust code through the same release-only cfg paths as opt.
    # Compile exec-platform Rust tools through those release-only cfg paths too.
    bazel test --compilation_mode=fastbuild --@rules_rust//rust/settings:extra_rustc_flag=-Cdebug-assertions=no --@rules_rust//rust/settings:extra_exec_rustc_flag=-Cdebug-assertions=no --cache_test_results=no --test_output=streamed --test_arg=--test //codex-rs:e2e-benchmarks

# Build and run Codex from source using Bazel.
# On Unix, use `[no-cd]` and `--run_under="cd $PWD &&"` to ensure Bazel runs

# the command in the current working directory.
[no-cd]
[unix]
bazel-codex *args:
    bazel run //codex-rs/cli:codex --run_under="cd $PWD &&" -- "$@"

[windows]
bazel-codex *args:
    bazel run //codex-rs/cli:codex --run_under='cd /d "{{ invocation_directory_native() }}" &&' -- @($args | Select-Object -Skip 1)

# Build and run the standalone code-mode host from source using Bazel.
[no-cd]
[unix]
bazel-code-mode-host *args:
    bazel run //codex-rs/code-mode-host:codex-code-mode-host --run_under="cd $PWD &&" -- "$@"

[windows]
bazel-code-mode-host *args:
    bazel run //codex-rs/code-mode-host:codex-code-mode-host --run_under='cd /d "{{ invocation_directory_native() }}" &&' -- @($args | Select-Object -Skip 1)

[no-cd]
bazel-lock-update:
    bazel mod deps --lockfile_mode=update

[no-cd]
[unix]
bazel-lock-check:
    {{ justfile_directory() }}/scripts/check-module-bazel-lock.sh

[windows]
bazel-lock-check:
    bazel mod deps --lockfile_mode=error; if ($LASTEXITCODE -ne 0) { Write-Error "MODULE.bazel.lock is out of date. Run 'just bazel-lock-update' and commit the updated lockfile."; exit 1 }

bazel-test:
    bazel test --test_tag_filters=-argument-comment-lint //... --keep_going

[no-cd]
[unix]
bazel-clippy:
    bazel_targets="$({{ justfile_directory() }}/scripts/list-bazel-clippy-targets.sh)" && bazel build --config=clippy -- ${bazel_targets}

[no-cd]
[unix]
bazel-argument-comment-lint:
    bazel build --config=argument-comment-lint -- $({{ justfile_directory() }}/tools/argument-comment-lint/list-bazel-targets.sh)

build-for-release:
    bazel build //codex-rs/cli:release_binaries

# Run the MCP server
mcp-server-run *args:
    cargo run -p codex-mcp-server -- {args}

# Regenerate the json schema for config.toml from the current config types.
write-config-schema:
    cargo run -p codex-core --bin codex-write-config-schema

# Regenerate vendored app-server protocol schema artifacts.
write-app-server-schema *args:
    cargo run -p codex-app-server-protocol --bin write_schema_fixtures -- {args}

[no-cd]
write-hooks-schema:
    cargo run --manifest-path {{ justfile_directory() }}/codex-rs/Cargo.toml -p codex-hooks --bin write_hooks_schema_fixtures

# Smoke-test the gateway promotion bundle generator.
[no-cd]
gateway-promotion-bundle-test:
    {{ justfile_directory() }}/scripts/test-create-gateway-promotion-bundle.sh

# Create a gateway promotion bundle skeleton.
[no-cd]
gateway-promotion-bundle-create *args:
    if [ "${1:-}" = "--" ]; then shift; fi; {{ justfile_directory() }}/scripts/create-gateway-promotion-bundle.sh "$@"

# Build the gateway Docker image from the repository root.
[no-cd]
gateway-docker-build:
    {{ justfile_directory() }}/scripts/ensure-gateway-docker-base.sh
    docker build -f {{ justfile_directory() }}/Dockerfile.gateway -t codex-gateway:local {{ justfile_directory() }}

# Build the app-server Docker image from the repository root.
[no-cd]
app-server-docker-build:
    {{ justfile_directory() }}/scripts/ensure-gateway-docker-base.sh
    docker build -f {{ justfile_directory() }}/Dockerfile.app-server -t codex-app-server:local {{ justfile_directory() }}

# Smoke-test the gateway Docker entrypoint wrapper.
[no-cd]
gateway-docker-entrypoint-test:
    sh {{ justfile_directory() }}/scripts/test-gateway-docker-entrypoint.sh

# Smoke-test the app-server Docker entrypoint wrapper.
[no-cd]
app-server-docker-entrypoint-test:
    sh {{ justfile_directory() }}/scripts/test-app-server-docker-entrypoint.sh

# Smoke-test the gateway Docker Compose wiring.
[no-cd]
gateway-docker-compose-test:
    sh {{ justfile_directory() }}/scripts/test-gateway-docker-compose.sh

# Start the gateway Docker Compose deployment from the repository root.
[no-cd]
gateway-docker-up:
    {{ justfile_directory() }}/scripts/ensure-gateway-docker-base.sh
    docker compose -f {{ justfile_directory() }}/docker-compose.gateway.yml up --build

# Start the gateway Docker Compose deployment with the built-in remote worker profile.
[no-cd]
gateway-docker-up-remote:
    {{ justfile_directory() }}/scripts/ensure-gateway-docker-base.sh
    CODEX_GATEWAY_RUNTIME=remote CODEX_GATEWAY_REMOTE_WEBSOCKET_URLS=ws://127.0.0.1:9001/v2 docker compose -f {{ justfile_directory() }}/docker-compose.gateway.yml --profile remote up --build

# Smoke-test the gateway promotion bundle checker.
[no-cd]
gateway-promotion-bundle-check-test:
    {{ justfile_directory() }}/scripts/test-check-gateway-promotion-bundle.sh

# Capture baseline health artifacts into a gateway promotion bundle.
[no-cd]
gateway-promotion-baseline-capture *args:
    if [ "${1:-}" = "--" ]; then shift; fi; {{ justfile_directory() }}/scripts/capture-gateway-promotion-baseline.sh "$@"

# Smoke-test the gateway promotion baseline capture helper.
[no-cd]
gateway-promotion-baseline-capture-test:
    {{ justfile_directory() }}/scripts/test-capture-gateway-promotion-baseline.sh

# Capture scoped /v1/events SSE artifacts into a gateway promotion bundle.
[no-cd]
gateway-promotion-events-capture *args:
    if [ "${1:-}" = "--" ]; then shift; fi; {{ justfile_directory() }}/scripts/capture-gateway-promotion-events.sh "$@"

# Smoke-test the gateway promotion event capture helper.
[no-cd]
gateway-promotion-events-capture-test:
    {{ justfile_directory() }}/scripts/test-capture-gateway-promotion-events.sh

# Capture scoped post-scenario /healthz artifacts into a gateway promotion bundle.
[no-cd]
gateway-promotion-health-capture *args:
    if [ "${1:-}" = "--" ]; then shift; fi; {{ justfile_directory() }}/scripts/capture-gateway-promotion-health.sh "$@"

# Smoke-test the gateway promotion health capture helper.
[no-cd]
gateway-promotion-health-capture-test:
    {{ justfile_directory() }}/scripts/test-capture-gateway-promotion-health.sh

# Import transcript, metrics, or log artifacts into a gateway promotion bundle.
[no-cd]
gateway-promotion-artifact-capture *args:
    if [ "${1:-}" = "--" ]; then shift; fi; {{ justfile_directory() }}/scripts/capture-gateway-promotion-artifact.sh "$@"

# Smoke-test the gateway promotion artifact capture helper.
[no-cd]
gateway-promotion-artifact-capture-test:
    {{ justfile_directory() }}/scripts/test-capture-gateway-promotion-artifact.sh

# Capture one gateway promotion traffic scenario artifact set.
[no-cd]
gateway-promotion-scenario-capture *args:
    if [ "${1:-}" = "--" ]; then shift; fi; {{ justfile_directory() }}/scripts/capture-gateway-promotion-scenario.sh "$@"

# Capture repeatable targeted-test evidence for the gateway promotion fault matrix.
[no-cd]
gateway-promotion-fault-matrix-capture *args:
    if [ "${1:-}" = "--" ]; then shift; fi; {{ justfile_directory() }}/scripts/capture-gateway-promotion-fault-matrix.sh "$@"

# Smoke-test the gateway promotion fault-matrix capture helper.
[no-cd]
gateway-promotion-fault-matrix-capture-test:
    {{ justfile_directory() }}/scripts/test-capture-gateway-promotion-fault-matrix.sh

# Smoke-test the gateway promotion scenario capture helper.
[no-cd]
gateway-promotion-scenario-capture-test:
    {{ justfile_directory() }}/scripts/test-capture-gateway-promotion-scenario.sh

# Validate a gateway promotion bundle layout.
[no-cd]
gateway-promotion-bundle-check *args:
    if [ "${1:-}" = "--" ]; then shift; fi; {{ justfile_directory() }}/scripts/check-gateway-promotion-bundle.sh "$@"

# Run the argument-comment Dylint checks across codex-rs.
[no-cd]
[unix]
argument-comment-lint *args:
    if [ "$#" -eq 0 ]; then \
      bazel build --config=argument-comment-lint -- $({{ justfile_directory() }}/tools/argument-comment-lint/list-bazel-targets.sh); \
    else \
      {{ justfile_directory() }}/tools/argument-comment-lint/run-prebuilt-linter.py "$@"; \
    fi

[no-cd]
argument-comment-lint-from-source *args:
    {{ python }} {{ justfile_directory() }}/tools/argument-comment-lint/run.py {args}

# Tail logs from the state SQLite database
[unix]
log *args:
    if [ "${1:-}" = "--" ]; then shift; fi; cargo run -p codex-state --bin logs_client -- "$@"

[windows]
log *args:
    $forwarded_args = @($args | Select-Object -Skip 1); if ($forwarded_args.Count -gt 0 -and $forwarded_args[0] -eq "--") { $forwarded_args = @($forwarded_args | Select-Object -Skip 1) }; cargo run -p codex-state --bin logs_client -- @forwarded_args
