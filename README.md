<h1>
  <img src="./public/clipsneko-logo.svg" align="center" height="100" alt="项目Logo" />
  ClipsNeko Linux Installer
</h1>

<p>
  <b>Our TUI installer.</b>
</p>

<p>
  <a href="https://github.com/zlicdt/clipsneko-installer/actions/workflows/build.yml"><img src="https://github.com/zlicdt/clipsneko-installer/actions/workflows/build.yml/badge.svg?branch=main" alt="CI status"></a>
</p>

> [!WARNING]
> The installation stage formats selected partitions. Run the installer only
> from the ClipsNeko Live ISO and verify every disk selection before confirming installation.

## Build

### Requirements

One x86_64 ClipsNeko Linux ~~or just Arch Linux if you like~~

On ClipsNeko Linux:

```console
sudo pacman -S base-devel rust gettext
```

### Compile

```console
git clone https://github.com/zlicdt/clipsneko-installer.git
cd clipsneko-installer
cargo build --release
```

The optimized binary is written to
`target/release/clipsneko-installer`.

The build script compiles all translation catalogs with `msgfmt`. Packaged
release builds must install the resulting catalogs under the GNU-standard
`/usr/share/locale/<locale>/LC_MESSAGES/clipsneko-installer.mo` path and install
`config/packages.list` as `/etc/clipsneko-installer/packages.list` on the Live
ISO. The installer exits at startup if that runtime configuration file is
missing.

## Development checks

Run the same core checks as CI before submitting a change:

```console
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
cargo build --release
```

CI additionally validates every PO catalog against
`po/clipsneko-installer.pot` with GNU gettext.

## Documentation

- [Design](docs/design.md)

## License

This project is licensed under the GPL 3.0 License.
