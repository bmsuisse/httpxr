import httpxr


def test_all_imports_are_exported() -> None:
    included_private_members = [
        "__description__", "__title__", "__version__",
    ]
    excluded_from_all = {"compat", "cli", "main"}
    assert httpxr.__all__ == sorted(
        (
            member
            for member in vars(httpxr).keys()
            if (
                not member.startswith("_")
                or member in included_private_members
            )
            and member not in excluded_from_all
        ),
        key=str.casefold,
    )

