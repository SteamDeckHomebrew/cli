use anyhow::{anyhow, Context, Ok, Result};
use log::debug;
use std::{path::PathBuf, process::Stdio};
use tokio::{
    fs::create_dir_all,
    io::{AsyncBufReadExt, BufReader},
    process::Command,
};
use users::{get_effective_gid, get_effective_uid};
use which::which;

async fn run_command(cmd: &mut Command) -> Result<()> {
    cmd.stdout(Stdio::piped());

    let mut child = cmd.spawn().expect("failed to spawn command");

    let stdout = child
        .stdout
        .take()
        .expect("child did not have a handle to stdout");

    let mut reader = BufReader::new(stdout).lines();

    // Ensure the child process is spawned in the runtime so it can
    // make progress on its own while we await for any output.

    let join_handle = tokio::spawn(async move { child.wait().await });

    while let Some(line) = reader.next_line().await? {
        println!("Line: {}", line);
    }

    join_handle.await?.map_err(|err| anyhow!(err)).and_then(
        |status| match status.success() {
            true => Ok(()),
            false => Err(anyhow!("child status was: {}", status))
        }
    )
}

pub fn ensure_availability() -> Result<()> {
    which("docker")
        .map(|_| Ok(()))
        .context("`docker` program not found. Make sure it is installed and in your $PATH. For more information visit https://docs.docker.com/desktop/troubleshoot/overview/")
        .and_then(|_| {
                std::process::Command::new("docker")
                    .arg("ps")
                    .status()
                    .context("Error while checking for Docker availability. Please run `docker ps` in your terminal and fix any errors that show up.")
            }
        )
        .map(|exit_status| exit_status.exit_ok().context("Docker is installed but doesn't seem to be available! Is the daemon running? For more information visit https://docs.docker.com/desktop/troubleshoot/overview/"))?
}

// docker build -f $PWD/backend/Dockerfile -t "$docker_name" .
pub async fn build_image(dockerfile: PathBuf, tag: String) -> Result<String> {
    let mut cmd = Command::new("docker");
    let full_command = cmd
        .arg("build")
        .arg("-f")
        .arg(dockerfile.canonicalize()?)
        .arg("-t")
        .arg(&tag)
        .arg(dockerfile.canonicalize()?.parent().unwrap());

    run_command(full_command).await?;

    Ok(tag)
}

// docker run --rm -i -v $PWD/backend:/backend -v /tmp/output/$plugin/backend/out:/backend/out --entrypoint /backend/entrypoint.sh "$docker_name"
pub async fn run_image(
    tag: String,
    binds: Vec<(String, String)>,
    run_as_root: bool,
    run_with_dev: bool,
) -> Result<()> {
    let mut cmd = Command::new("docker");
    let mut command_with_default_args = cmd.arg("run").arg("--rm");

    if !run_as_root {
        command_with_default_args = command_with_default_args.arg("--user").arg(format!(
            "{}:{}",
            get_effective_uid(),
            get_effective_gid()
        ));
    }

    if run_with_dev {
        command_with_default_args = command_with_default_args
            .arg("-e")
            .arg("RELEASE_TYPE=development")
    } else {
        command_with_default_args = command_with_default_args
            .arg("-e")
            .arg("RELEASE_TYPE=production")
    }

    let mut dynamic_args: Vec<String> = vec![];

    for bind in binds {
        // Pre-create bind-mounted directories as the current user to ensure writability.
        // Otherwise they are created by the Docker daemon, which may be a different user.
        create_dir_all(&bind.0).await?;

        let bindstr = format!("{}:{}", bind.0, bind.1);
        dynamic_args.push("-v".into());
        dynamic_args.push(bindstr);
    }

    let full_command = command_with_default_args.args(dynamic_args).arg(tag);
    debug!("full_command: {full_command:?}");
    run_command(full_command).await?;

    Ok(())
}
