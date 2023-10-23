# Decky Plugin CLI

CLI to aid in development of plugins for [Decky Loader](https://github.com/SteamDeckHomebrew/decky-loader).
Used by the [Decky Plugin Template](https://github.com/SteamDeckHomebrew/decky-plugin-template).

## Requirements

A nightly version of rust is required to support the use of pre-release language features.
Here's an easy method to get one:

1. [Install `rustup`.](https://www.rust-lang.org/tools/install)
2. Use `rustup` to install a nightly version of `rust`:

    ```shell
    rustup toolchain install nightly
    ```

### Ubuntu

These additional dependencies are required to build:

```shell
apt install pkg-config libssl-dev
```

## Development

### Build & Deploy

Build CLI for debugging (output to `./target/debug/decky`):

```shell
cargo +nightly build
```

You can now copy the built binary in to a plugin project for testing (command assumes Decky CLI and your plugin have been cloned alongside each other):

```shell
cp target/debug/decky ../your-decky-plugin/cli/decky
```

Or, if you're planning lots of changes, you could symlink the plugin's binary to your build (command assumes Decky CLI and your plugin have been cloned alongside each other):

```shell
ln -fs ../../cli/target/debug/decky ../your-decky-plugin/cli/decky
```

### Logging

Logging uses [`flexi_logger`](https://docs.rs/flexi_logger/latest/flexi_logger/) and is controlled via the `RUST_LOG` environment variable.

For example, to get debug level logs across the CLI:

```shell
RUST_LOG=DEBUG $CLI_LOCATION/decky plugin build $(pwd)
```

### Local Release

Build CLI for release (output to `./target/release/decky`):

```shell
cargo +nightly build --release
```
