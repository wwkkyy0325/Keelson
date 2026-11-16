# Backlog

Cases that are not scheduled yet, with the evidence and the first step.

## Sampling parameters in a profile

`temperature` (and the rest of the sampling knobs) cannot be set from a rig
profile: `keel-policy`'s `ModelSection` is `deny_unknown_fields` and has no
such field, so a profile that writes `temperature = 0.7` fails to load and the
whole rig disappears from the picker. `figure-live.toml` carried the field
and was broken this way until it was removed.

First step: add `temperature: Option<f32>` to `ModelSection` in the Keel
repository, forward it to the model route, and cover it with a contract test.
The rig files then get their comments' promises back.

## Profile inheritance

`ProfileSection` already parses `extends`, but `from_profile_str` refuses it
with `profile inheritance is not supported in M1`. Every live profile repeats
the same 18-line `[model]` and `[context]` block, so changing an endpoint or a
model id is five edits.

First step: decide whether `extends` merges sections field by field or only
`[model]`; then implement it in Keel with a contract test. Until then the
duplication is deliberate — one file you can read beats a merge you cannot.
