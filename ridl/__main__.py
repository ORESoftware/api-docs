"""Allow `python3 -m ridl` as well as `python3 -m ridl.cli`."""

from .cli import main

if __name__ == "__main__":
    main()
