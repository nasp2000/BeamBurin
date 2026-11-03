# Contributing

Thanks for helping. BeamBurin is a small, focused project: a feature is done
only when it has logic, UI, persistence, error handling and tests.

## Before you start

- Open an issue for a behaviour change or a new dependency, so the scope is
  clear before code is written.
- Keep the scope: BeamBurin prepares jobs and never controls machines. Do not
  add serial/USB, streaming, homing, `$` settings or connection profiles.
- Target Rust 1.95 (2024 edition). Dependencies must be MIT-compatible and are
  checked by `cargo-deny`.

## Rules

- **Tests:** add Rust unit or integration tests for the change. UI behaviour
  uses the headless `egui_kittest` harness in `src/app.rs`; no GPU is needed.
- **Formatting:** `cargo fmt --all` before committing. CI runs
  `cargo fmt --all --check`, clippy with `-D warnings`, the test suite,
  `cargo-deny`, actionlint and the headless start test.
- **Files:** validate before saving, write atomically, and never lose or corrupt
  a project. Warn about unsupported imported or exported elements.
- **Identifiers:** layers and objects are identified by UUID, never by name.
- **Third-party code and assets:** do not paste code, icons, fonts or text from
  another project unless its licence is compatible and the notice is added. Run
  `python3 scripts/third_party_notices.py` after a dependency change and commit
  the updated `THIRD_PARTY_NOTICES.md`.
- **Names:** do not add names, logos or screenshots of commercial products to
  the code, tests, assets, documentation or commit messages. Describe formats
  and behaviour generically. The `no_product_names` test enforces this.

## Developer Certificate of Origin

Every commit must be signed off. Use `git commit -s`, which adds:

```
Signed-off-by: Your Name <you@example.com>
```

By signing off you certify the Developer Certificate of Origin, version 1.1:

```
By making a contribution to this project, I certify that:

(a) The contribution was created in whole or in part by me and I have the right
    to submit it under the open source licence indicated in the file; or

(b) The contribution is based upon previous work that, to the best of my
    knowledge, is covered under an appropriate open source licence and I have
    the right under that licence to submit that work with modifications,
    whether created in whole or in part by me, under the same open source
    licence (unless I am permitted to submit under a different licence), as
    indicated in the file; or

(c) The contribution was provided directly to me by some other person who
    certified (a), (b) or (c) and I have not modified it.

(d) I understand and agree that this project and the contribution are public and
    that a record of the contribution (including all personal information I
    submit with it, including my sign-off) is maintained indefinitely and may be
    redistributed consistent with this project or the open source licence(s)
    involved.
```

## Pull request checklist

- [ ] The change is scoped and has tests.
- [ ] `cargo fmt --all --check` passes.
- [ ] The UI has an English label, tooltip and an accessible name for new
      controls.
- [ ] `THIRD_PARTY_NOTICES.md` is current if dependencies or assets changed.
- [ ] No names, logos or assets of commercial products were added.
- [ ] Commits are signed off.
