# Task contract

## Source

Message 1 (2026-07-28):

> on the rustpix gui, there is a hard cap of 2000 bins for the tof, which needs to be uncapped as the instrument scientist just ask to have 10,000 bins. In this session, we will
>
> * evaluate how to handle this request
> * implemnt the uncapping
> * cut a new patch release and go thorugh the relesae process
> * update the deployment on analysis (done by me manually by updaing the gui app from the PyPI source)

Message 2 (2026-07-28, mid-turn):

> helpful info, the machine running rustpix by the isntrument scientist has 1TB of ram

## Rules (enforced by the stop gate)

- One checkbox per requirement, quoting the Source's own words. Every
  imperative sentence in the Source must map to a requirement line.
- Checkboxes must use exactly `- [ ]` — other bullet styles are
  invisible to the gate.
- There are four states and no others. An unrecognized state — `[~]`,
  `[o]`, `[WIP]`, anything invented — blocks; it does not park an item.
- `[x]` requires evidence ON the line: the check command in backticks
  and what it showed. The gate blocks `[x]` lines with no backticked
  command; an unfilled `<placeholder>` in backticks does not count.
- If an item's recorded evidence is later invalidated (a retraction, a
  failed re-check), flip it back to `- [ ]` — a checked line with dead
  evidence is a false report.
- Dropping an item requires the user's explicit approval of that named
  item: `- [-] DROPPED: <item> (approved: "<user's words>")`. The quoted
  words must themselves address the dropped item — a generic "proceed"
  or "sounds good" is never approval. The gate blocks `[-]` lines with
  no `(approved:` quote.
- Blocked on the user? Mark it `- [?] <item> — BLOCKED ON USER:
  <specific question>`, finish everything not blocked, then end the turn
  asking about any `[?]` item you have not already put to the user. A
  question the user has declined to answer is not re-asked: leave the
  item `[?]`, say so in the report, and let the user reopen it.
- Never delete or reword a requirement line. Discovered work is ADDED
  as new lines, not swapped in.
- Commit this file unless the user objects — tampering must show in
  git diff.

## What a checked box does and does not mean

`[x]` means done and evidenced: a command was run and its result is on
the line. It does not mean correct. A fully evidenced result can still be
wrong — the wrong metric, a confounded comparison, a check that cannot
fail. The gate audits process; only independent review audits truth. Read
a green contract as "nothing was skipped or quietly dropped", never as
"this is right".

## Coverage check

Sentence-by-sentence mapping of the Source. Every directive sentence maps
to a numbered requirement.

| Source sentence | Maps to |
| --- | --- |
| "on the rustpix gui, there is a hard cap of 2000 bins for the tof, which needs to be uncapped as the instrument scientist just ask to have 10,000 bins." | R2, R3 (the change + the 10,000-bin acceptance criterion it implies) |
| "In this session, we will" | (framing, no directive) |
| "* evaluate how to handle this request" | R1 |
| "* implemnt the uncapping" | R2, R3 |
| "* cut a new patch release and go thorugh the relesae process" | R5, R6, R7, R8 |
| "* update the deployment on analysis (done by me manually by updaing the gui app from the PyPI source)" | R9 — explicitly the USER's action, not mine. My obligation is to make the PyPI artifact exist and hand over the exact upgrade command. |
| "helpful info, the machine running rustpix by the isntrument scientist has 1TB of ram" | R1 (sizing input to the evaluation), R3 (fixes the headroom target) |

Implied acceptance criteria not stated as sentences but required for the
work to be real: R4 (no regression — the existing suite still passes).

## Requirements

- [x] R1: "evaluate how to handle this request" — a written evaluation covering the memory cost of the dense `Hyperstack3D` at 10,000 bins on the real VENUS 514x514 detector, whether 10,000 bins is physically meaningful given `tof_max`, the 1 TB RAM headroom, and the recommended approach — check: evaluation is present in the session report AND the chosen ceiling is justified in a code comment — `grep -n "MAX_TOF_BINS" rustpix-gui/src/ui/control_panel.rs` → hits at lines 8 (import) and 1789/1815 (both DragValue ranges); the 15-line justification lives on the constant in `rustpix-gui/src/histogram/mod.rs:14-29`. 79-agent audit confirmed: 21.14 GB/stack at 10k bins, up to 4 concurrent copies (hit + neutron + rebuild transient + HDF5 export) ≈ 84.5 GB peak = 8.5% of 1 TB.
- [x] R2: "there is a hard cap of 2000 bins for the tof, which needs to be uncapped" — the `range(10..=2000)` hard cap is gone from BOTH the hits and neutrons TOF bin controls — check: `grep -c "10\.\.=2000" rustpix-gui/src/ui/control_panel.rs` → `0`
- [x] R3: "the instrument scientist just ask to have 10,000 bins" — 10,000 is an accepted value for both hit and neutron TOF bins, with headroom well beyond it — check: `cargo test -p rustpix-gui tof_bin` → `test result: ok. 3 passed` (`tof_bin_range_admits_ten_thousand_bins`, `tof_bin_ceiling_stays_under_the_25ns_information_limit`, `tof_binning_is_correct_at_ten_thousand_bins`)
- [x] R4: (implied) the change does not regress existing behaviour — check: `cargo test --workspace --exclude rustpix-python` → `CARGO TEST EXIT: 0`, 17 binaries all `test result: ok`, 134 tests, 0 failed; `cargo clippy --workspace --all-targets -- -D warnings` → `CLIPPY EXIT: 0`; `cargo fmt --all -- --check` → `FMT CLEAN`
- [x] R5: "cut a new patch release" — version bumped 1.1.2 -> 1.1.3 across every file that carries the version, in sync — check: `python scripts/version.py check` → `All versions are in sync!` with source of truth 1.1.3, all 8 crate Cargo.toml files ✓ on workspace inheritance; `python scripts/version.py show` → Cargo/pyproject/gui-pyproject all 1.1.3; `grep -n "rustpix-gui==" pyproject.toml` → `gui = ["rustpix-gui==1.1.3"]`; `cargo update -w` regenerated Cargo.lock to 1.1.3 for all 8 rustpix crates
- [x] R6: "cut a new patch release" — CHANGELOG.md has a 1.1.3 entry describing the uncapping — check: `grep -n "## \[1.1.3\]" CHANGELOG.md` → `10:## [1.1.3] - 2026-07-28`
- [x] R7: "go thorugh the relesae process" — the release lands on `main` via the repo's normal path (branch -> PR -> merge), not a direct push — check: `gh pr view 129 --json state,mergedAt,mergeCommit` → `state=MERGED mergedAt=2026-07-28T18:47:50Z mergeCommit=17593bb80c73121ae2a6004d9c8cf982458622d4`; all 9 CI checks passed before merge (`gh pr checks 129` → Check/Clippy/Coverage/Documentation/Format/Python bindings/Test macos+ubuntu+windows all `pass`)
- [x] R8: "go thorugh the relesae process" — tag `v1.1.3` pushed and the release workflow completes green, including the PyPI publish job — check: `gh run view 30389105386` → `release.yml conclusion: success`, all 12 jobs success incl. `Publish to crates.io: success`, `Publish to PyPI: success`, `Create GitHub Release: success`; PyPI JSON API lists `rustpix` 1.1.3 with 4 wheels + sdist
- [x] R8b: (discovered during review) the GUI does NOT ship via release.yml — it is built and published as the separate PyPI package `rustpix-gui` by `.github/workflows/gui.yml`, which fires on the same `v*` tag but has its own failure surface. R8 can pass green while the GUI fix never reaches PyPI, which would make R9 impossible. — check: `gh run view 30389107475` → `gui.yml conclusion: success`, all 6 jobs success incl. `Publish GUI to PyPI: success`; PyPI JSON API lists `rustpix-gui` 1.1.3 with `rustpix_gui-1.1.3-py3-none-manylinux_2_28_x86_64.whl`. Downloaded that wheel and confirmed it carries a 26 MB `ELF 64-bit LSB pie executable, x86-64` at `rustpix_gui-1.1.3.data/scripts/rustpix-gui` containing the strings `" in memory"` and `" Exceeds the "`, which exist only in the new `render_hyperstack_size_hint` — so the shipped artifact really has the change.
- [x] R9: "update the deployment on analysis (done by me manually by updaing the gui app from the PyPI source)" — USER-OWNED action. My obligation: PyPI has an installable 1.1.3 artifact and the user is handed the exact upgrade command — check: R8b's verified wheel is the installable artifact; upgrade command `pip install --upgrade rustpix-gui==1.1.3` stated in the final report. The install itself is the user's to run, by their own framing.
