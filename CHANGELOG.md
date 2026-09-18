# Changelog

Notable changes to Verditum, newest first. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## [Unreleased]

The initial release is in development. It includes:

- Open UYAP `.udf` documents and display text, inherited styles, tables, merged
  cells, images, headers, footers, and explicit page breaks.
- Light and dark themes and manual zoom controls.
- Offline UDF signature-integrity verification with per-signer details and
  separate unsigned, invalid, and unsupported results. Certificate trust,
  revocation, and timestamps are not checked.
- Fit-to-width control, enabled by default, that adjusts the document to the
  available window width and can be toggled off or on.
- Export the open document to PDF with embedded subset fonts, pagination, and
  repeated headers and footers, then open it in the desktop PDF application or
  the mobile share sheet, logging any rendering limitations.
- Desktop auto-update: at startup the app checks the latest GitHub release and,
  if the user agrees, installs a newer signed version and restarts.
- Apache-2.0 license.

[Unreleased]: https://github.com/ecmel/verditum/commits/HEAD
