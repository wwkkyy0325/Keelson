# M4w — read_file line numbers

Goal: let the model see absolute line numbers without changing the raw file
content or the edit contract.

## Keel

- `read_file` accepts `line_numbers` (boolean, default false).
- Numbered output is `%5d | line`, absolute to the file, and works with a
  range.
- The ledger records raw bytes; the truncation marker is appended after
  numbering and is not numbered.
- ADR 0027 records the contract.

## Host

- The persona says the prefix is not part of the file and must not appear in
  edit text.

## Verification

- `read_file_line_numbers_are_absolute`: a range starting at line 2 prints
  `    2 | two`.
- `read_file_rejects_a_non_boolean_line_numbers`.
- The default read test still returns raw content.
- Keel: 115 lib tests, 32 tool tests (+2), clippy. Keelson: 51 lib tests,
  polyglot eval 11.35 s, desktop 28.

## Not in M4w

- The default remains raw text.
- `edit_file` and `multi_edit` never accept numbered text.
