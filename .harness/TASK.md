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
- [ ] R5: "cut a new patch release" — version bumped 1.1.2 -> 1.1.3 across every file that carries the version, in sync — check: `python scripts/version.py check` reports in sync and shows 1.1.3
- [ ] R6: "cut a new patch release" — CHANGELOG.md has a 1.1.3 entry describing the uncapping — check: `grep -n "## \[1.1.3\]" CHANGELOG.md`
- [ ] R7: "go thorugh the relesae process" — the release lands on `main` via the repo's normal path (branch -> PR -> merge), not a direct push — check: `gh pr view --json state,mergedAt` shows merged
- [ ] R8: "go thorugh the relesae process" — tag `v1.1.3` pushed and the release workflow completes green, including the PyPI publish job — check: `gh run list --workflow=release.yml` shows success for the v1.1.3 tag AND `pip index versions rustpix` (or the PyPI JSON API) lists 1.1.3
- [ ] R9: "update the deployment on analysis (done by me manually by updaing the gui app from the PyPI source)" — USER-OWNED action. My obligation: PyPI has an installable 1.1.3 artifact and the user is handed the exact upgrade command — check: R8's PyPI evidence + the command is stated in the final report
