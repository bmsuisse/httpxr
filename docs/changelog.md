# Changelog

All notable changes to httpxr will be documented here.

---

## 0.28.1 (2025-02-17)

### Fixed

- `Response.url` now correctly returns the request URL when using the fast path
- `raise_for_status()` now works correctly on fast-path responses

### Changed

- Migrated repository to [GitHub](https://github.com/bmsuisse/httpxr)

---

## 0.28.0

### Added

- Initial public release
- Full httpx API compatibility
- `gather()` for concurrent batch requests
- `paginate()` for auto-pagination
- Raw API (`get_raw()`, `post_raw()`, etc.) for maximum-speed dispatch
- Optional CLI via `httpxr[cli]`
- Zero runtime Python dependencies

