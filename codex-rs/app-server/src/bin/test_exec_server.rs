use anyhow::Context;
use anyhow::Result;
use anyhow::bail;
use codex_exec_server::ExecServerRuntimePaths;

#[tokio::main]
async fn main() -> Result<()> {
    let listen_url = parse_listen_url()?;
    let current_exe = std::env::current_exe().context("failed to resolve current executable")?;
    let runtime_paths =
        ExecServerRuntimePaths::new(current_exe, /*codex_linux_sandbox_exe*/ None)
            .context("failed to configure exec-server runtime paths")?;

    codex_exec_server::run_main(&listen_url, runtime_paths)
        .await
        .map_err(anyhow::Error::msg)
}

fn parse_listen_url() -> Result<String> {
    let mut args = std::env::args().skip(1);
    let first = args.next();
    let flag = match first.as_deref() {
        Some("exec-server") => args.next(),
        other => other.map(str::to_string),
    };

    if flag.as_deref() != Some("--listen") {
        bail!("expected --listen <url>");
    }

    let listen_url = args.next().context("expected --listen <url>")?;
    if args.next().is_some() {
        bail!("unexpected extra argument");
    }
    Ok(listen_url)
}
