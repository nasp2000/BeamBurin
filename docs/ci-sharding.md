# CI test sharding — measurement and plan

The suite already runs through `cargo-nextest` on two platforms. This note
records the measured effect of splitting it into two shards per platform and
the design to adopt when the suite grows enough to pay for the extra jobs.

## Current pipeline

`Build` (`.github/workflows/build.yml`) runs on every push and pull request:

- `quality`: formatting, clippy, doctests.
- `test`: one job per platform (Windows x86_64 and Linux aarch64), each running
  the full suite with `cargo nextest run --workspace --locked --target <target>`.
- `build`: produces both artifacts after the quality, deny, msrv and test jobs.

`.config/nextest.toml` keeps `fail-fast = false`, prints failures at the end,
suppresses successful test lines and treats a test over two minutes as a hang.
Doctests stay in `quality` because nextest does not run them.

## Baseline (run 44432279231, tip 13de5ae)

| Job | Duration | Test command |
|---|---:|---|
| Tests linux-aarch64 | 115 s | 65.7 s: compile 36.3 s + 29.4 s running 1407 tests |
| Tests windows-x86_64 | 137 s | 31.8 s running the suite |
| Whole workflow | 8.52 min | dominated by the two release builds |

Setup (checkout, toolchain, apt, nextest, cache restore) is about 49 s per
Linux job. Compilation of the test binaries is 36 s. The test execution itself
is under 32 s on both runners.

## Two-shard experiment (run 44456000584, tip 6f5abac)

The test matrix was doubled to two shards per platform, each running
`cargo nextest run --workspace --locked --target <target> --partition count:<i>/2`.

| Job | Duration | Tests run | Execution |
|---|---:|---:|---:|
| Tests windows-x86_64 (1/2) | 175 s | 706 | 32.1 s |
| Tests windows-x86_64 (2/2) | 142 s | 700 | 15.0 s |
| Tests linux-aarch64 (1/2) | 110 s | 706 | 24.3 s |
| Tests linux-aarch64 (2/2) | 97 s | 701 | 9.9 s |
| Whole workflow | 6.92 min | — | — |

Reading:

- The **test phase** (the time the build jobs wait for all test jobs) went from
  **137 s to 175 s** on Windows, a 38 s regression, and from 115 s to 110 s on
  Linux, within noise. The 6.92 min total is not attributable to sharding: the
  release builds happened to take 235 s/218 s in that run against 368 s/245 s
  in the baseline.
- `count` partitions are unbalanced because slow UI tests cluster: Linux shard
  1 took 24.3 s while shard 2 took 9.9 s; Windows 32.1 s against 15.0 s.
- Every shard repeats checkout, toolchain, apt, nextest installation, cache
  restore and, most importantly, **36 s of test-binary compilation**. Two
  shards therefore cost roughly 2× runner minutes for no wall-clock gain at the
  current size.

## Decision

The sharded matrix was reverted. The full suite keeps running once per platform
on every push, which is faster and cheaper today. The nextest configuration is
kept because it improves failure reporting and guards against hangs.

## When to revisit

Adopt sharding when either condition holds for two consecutive `Build` runs:

- the single-platform nextest **execution** summary is above 90 s; or
- the `test` job is above 4 minutes on both platforms.

At that point, compare the shard wall-clock with the single job and keep it only
if the test phase drops by at least 30 s and total runner minutes stay below
1.5× the single-job cost.

## Proposed design for the next step

1. **Build the test binaries once per platform.** A `test-build` job runs
   `cargo nextest archive --workspace --locked --target <target>
   --archive-file target/nextest/archive.tar.zst` and uploads the archive with
   one-day retention. If the archive is larger than 250 MiB, stop and keep the
   single job: download time would dominate.
2. **Run the shards from the archive.** A matrix of `platform × shard` (two
   shards at 90 s, four at 180 s) downloads the archive and runs
   `cargo nextest run --archive-file target/nextest/archive.tar.zst
   --partition hash:<i>/<n>`. Use `hash` rather than `count`: the measured
   `count` split was 24.3 s against 9.9 s on Linux.
3. **Keep the gates unchanged.** `build` still needs the aggregate `test` job;
   doctests stay in `quality`; the ignored corpus scenarios stay in
   `corpus.yml`; every shard runs the full platform suite partition, with no
   path filtering.
4. **Measure the result.** Record the per-shard execution summary and the job
   durations in this file. Revert if the test phase does not improve by at
   least 30 s or if the runner-minute multiplier exceeds 1.5×.

## Reproduce the experiment

```sh
# baseline: one job per platform
cargo nextest run --workspace --locked --target <target>

# experiment: half the tests, still compiling the whole test suite
cargo nextest run --workspace --locked --target <target> \
  --partition count:1/2
```

Runs: baseline `44432279231`; two-shard experiment `44456000584` (both on
`nasp2000/BeamBurin`).
