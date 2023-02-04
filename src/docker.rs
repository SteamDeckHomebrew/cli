use anyhow::Result;
use std::{path::PathBuf, process::Stdio};
use tokio::{
    io::{AsyncBufReadExt, BufReader},
    process::Command,
};
use users::{get_effective_gid, get_effective_uid};

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
    tokio::spawn(async move {
        let status = child
            .wait()
            .await
            .expect("child process encountered an error");

        println!("child status was: {}", status);
    });

    while let Some(line) = reader.next_line().await? {
        println!("Line: {}", line);
    }

    Ok(())
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
pub async fn run_image(tag: String, binds: Vec<(String, String)>, run_as_root: bool) -> Result<()> {
    let mut cmd = Command::new("docker");
    let mut command_with_default_args = cmd.arg("run").arg("--rm");

    if !run_as_root {
        command_with_default_args = command_with_default_args.arg("--user").arg(format!(
            "{}:{}",
            get_effective_uid(),
            get_effective_gid()
        ));
    }

    let mut dynamic_args: Vec<String> = vec![];

    for bind in binds {
        let bindstr = format!("{}:{}", bind.0, bind.1);
        dynamic_args.push("-v".into());
        dynamic_args.push(bindstr);
    }

    let full_command = command_with_default_args.args(dynamic_args).arg(tag);
    run_command(full_command).await?;

    Ok(())
}
