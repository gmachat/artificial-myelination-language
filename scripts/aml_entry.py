"""Entrypoint used by standalone AML executables."""

from aml.__main__ import main


if __name__ == "__main__":
    raise SystemExit(main())
