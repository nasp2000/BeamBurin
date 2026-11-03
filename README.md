# BeamWright

Vector drawing and preparation of laser engraving and cutting jobs. BeamWright
turns drawings and images into a job file that you load into the sender of your
choice. It is portable, works offline and **never controls a machine**.

Status: pre-1.0. CI builds and tests every push; there is no release channel or
tagged version yet. The binary is a single executable on Windows x86_64 and
Linux x86_64/aarch64.

## What it does

- Draw and edit shapes, paths, text, images, layers and groups.
- Import SVG, DXF, HPGL, PDF and raster images; export SVG, DXF, HPGL, EPS, PDF,
  PNG and G-code.
- Assign operations to layers or selections with speed, power, passes, kerf,
  tabs, leads, fill patterns, offset and boolean tools.
- Prepare raster work with dithering, tone curves, vectorisation and centreline
  tracing.
- Preview the job, estimate its duration, and keep reusable material, machine and
  operation profiles.
- Automate the editor through the local MCP bridge if you want to.

## What it does not do

- No serial/USB connection, streaming, homing, `$` settings, work-coordinate
  systems, start/stop or connection profiles.
- No installer, service, registry entry or administrator rights.
- macOS is out of scope.

The deliverable is a file. The sender and the machine remain your responsibility.

## Build and test

Rust 1.95 or newer. The project uses the 2024 edition and pins dependencies.

```sh
cargo build --release --locked
cargo fmt --all --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
```

CI is the source of truth: formatting, clippy, tests (including headless UI tests
with `egui_kittest`), `cargo-deny`, actionlint, an Xvfb smoke test and the
Windows/Linux-aarch64 builds. There is no release workflow.

## Licences

BeamWright is licensed under the **MIT License or the Apache License 2.0**, at
your option. See `LICENSE-MIT` and `LICENSE-APACHE`.

The binary embeds fonts and Rust crates under their own licences. The complete
list and licence texts are in `THIRD_PARTY_NOTICES.md`, inside the application
under **Help → Licenses**, and in the output of:

```sh
BeamWright --licenses
```

## Name and affiliation

This is an independent project. It is not affiliated with, endorsed by or
sponsored by any other software vendor, and it does not use their names, logos
or assets. File-format names are used only to describe interoperability.

## Contributing

See `CONTRIBUTING.md`. Contributions are accepted under the Developer
Certificate of Origin; sign your commits with `git commit -s`.
