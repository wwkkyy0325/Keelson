# messy-workspace

A large file with the kind of damage a reformatting edit leaves behind.

- `app/engine.py` (1274 lines): the bodies of rules 010-040 sit at the `def`
  column, and five later methods have one line indented a column too far.
  Python reports `expected an indented block`; after that is fixed it reports
  `unindent does not match any outer indentation level`.
- `src/Engine.java` (570 lines): one method lost the brace that closes its
  `if`, and another gained a stray brace after its opening line. `javac`
  reports `illegal start of expression` and `<identifier> expected` in
  cascade.

The Python project is detected from `tests/`. Java is outside the four
detected ecosystems, so its check is declared in `.keelson/checks.toml`;
`javac` runs with English diagnostics so the output does not depend on the
machine locale.

`tests/test_engine.py` and `src/Main.java` are the reference behavior and must
not change.

Live eval: `tests/messy_eval.rs`.
