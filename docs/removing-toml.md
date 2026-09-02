# Removing TOML as the programme format

**Written**: 2026-09-02, after surveying the code but before changing any of it.
For whoever does the work — this is a plan, not a record.

The operator has asked for this across several sessions. The handover of
2026-09-02 has the standing quotes; question 11 of the same day settled the two
config files: *"there is no toml, there is only the database or the operating
systems key store."*

## What the survey found

**It is mostly deletion, not extraction.** The wizard's `Draft` carries six
fields — name, start, pattern, primary, weekdays, shape. `document.rs` carries
thirteen sections. The difference is everything only a hand-written file could
ever have said, and it goes without needing a replacement.

**Four pieces of machinery become unnecessary rather than needing to survive.**
This is the part worth knowing before starting, because each looks like work and
is not:

1. **`refuse_unused` disappears, at all eight call sites.** It exists because a
   flat document could carry a field its template refuses — an anchor on a test,
   an entry test on a linear, an opening on a block, a gating role on an SBS
   cycle. A typed `Shape` enum with a variant per template has no field for
   them, so every one of those checks becomes unrepresentable. The type system
   replaces the validation rather than the validation being moved.

2. **Inherited fills go.** `fills_over(inherited)` exists so a hand-written test
   document can name only what changes. The wizard asks all seventeen slots
   unconditionally, for every template — `for slot in SlotId::ALL` — so it
   always produces complete fills and there is nothing to fall back to.

3. **Stated-versus-derived interruptions go.** `Document::programme` chooses
   between interruptions written into the document and those derived from the
   schedule. With no document there is no stated case, so they are always
   derived. Decision 0018 then deletes the rest of this.

4. **The `[parameters]` section goes.** `prescribing::add` falls back to the
   stored set when a document states none; with no document it always reads the
   store.

## The shape to build

Replace `document.rs` and `draft.rs` with one module over typed inputs:

```rust
pub struct Authored {
    pub name: ProgrammeName,
    pub start: Date,
    pub pattern: PrimaryPattern,
    pub primary_exercise: Exercise,
    pub weekdays: Weekdays,
    pub shape: Shape,
}

pub enum Shape {
    Test  { reps: RepCount, target: TestTarget },
    Linear{ gating: SessionRole, weeks: u32, anchor: Anchor, opening: Option<Kg> },
    Block { gating: SessionRole, weeks: u32, anchor: Anchor, entry_test: Option<EntryTest> },
    Sbs   { anchor: Anchor },
}

pub fn programme(
    authored: Authored,
    fills: SlotFills,
    interruptions: &[Skip],
    zone: TimeZone,
    parameters: &GenerationParameters,
) -> Result<Programme, AuthoringError>
```

The `Shape` variants are where point 1 above lands: `Sbs` has no gating role
because the chart states it, `Test` has no anchor because measuring is what it
is for, `Linear` has no entry test and `Block` has no opening.

**An assembler is needed for `SlotFills`.** It has a field per slot and two of
them (`plyometric`, `power`) are `Fill<StaticFill>` where the rest are
`Fill<Exercise>`, so the wizard's answers cannot be a homogeneous `Vec` of
pairs. Build the struct with a match over `SlotId`, in the order the wizard
already asks.

## The edits

| file | change |
|---|---|
| `infrastructure/src/programme/document.rs` | delete, 1,155 lines |
| `infrastructure/src/programme/draft.rs` | delete, 568 lines |
| `infrastructure/src/programme/mod.rs` | export the new module |
| `cli/src/wizard.rs` | build `Authored` and `SlotFills` directly; drop `render`, the file write and `--into` |
| `cli/src/prescribing.rs` | `add` takes typed inputs, not a path; `derived_interruptions` takes a window rather than a `Document` |
| `cli/src/main.rs` | `programme add` loses its `path` and `--into` arguments |
| `Cargo.toml`, `infrastructure/Cargo.toml` | drop the `toml` dependency |

**Four integration tests author through documents** and would build programmes
directly instead: `standalone_test.rs`, `scheduled_programme.rs`,
`authored_data.rs`, `sbs_cycle.rs`. The round-trip test added on 2026-09-02,
`the_wizard_s_sbs_output_authors_a_cycle`, goes with the format it tests.

## Then, separately

`credentials.rs` and `settings.rs` still use `toml`, and the operator has said
both go — credentials to the OS keystore, settings to the database. That is a
second change with its own risk (a new dependency, and credential handling),
and it should not ride along with this one.

Note against roadmap open question 4, which rejected an OS keyring as the *only*
mechanism because it needs an unlocked session and so fails under cron and over
SSH. The operator has since chosen the keystore. That trade is real and is his
to have made; record it rather than re-litigating it.

## What this does not touch

Decisions 0027 and 0028 delete much of what survives here — `Anchor`, `Entry`,
`TestTarget::Declared`, and the parameter types that are really the programme's.
That is not a reason to wait: what they delete is the small typed core, not the
text layer, and doing this first means those changes land in a codebase 1,700
lines smaller.
