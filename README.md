# Verditum

Document Viewer for UYAP

See the [changelog](CHANGELOG.md) for notable changes.

## Development

Install Node.js 22.12+ and the Rust/Tauri prerequisites for your platform, then
run:

```sh
npm ci
npm run debug
```

`npm run build` checks TypeScript and builds the frontend. `npm test` runs the
tests; install their browser once with `npx playwright install chromium`.
`npm run test:coverage` writes an HTML report to `coverage/`.

## UDF formatting

The viewer renders `.udf` documents locally, including styled text, tables,
images, headers and footers, page layout, and explicit page breaks, with zoom,
dark mode, and printing.

This is a best-effort renderer, not an exact reproduction of UYAP Editor.
Automatic pagination, page numbers, custom tab stops, automatic list markers,
and background images are not fully supported, and installed fonts can affect
line wrapping. Compare important output with UYAP Editor.

## Signature integrity

Opening a UDF verifies its `sign.sgn` signature against `content.xml`, offline,
and shows whether the document is unsigned, validly signed, invalid, or uses an
unsupported format, with each signer's certificate subject and issuer. RSA
(SHA-256/384/512) and ECDSA P-256/P-384 (SHA-256/384) signatures are supported.

Certificate trust chains, validity periods, revocation, and timestamps are **not
checked**, so certificate names are not proof of identity. Only `content.xml` is
covered, and exported PDFs do not carry the signature.

## PDF export

The share button exports the open document to PDF with embedded fonts,
pagination, and repeated headers and footers, then opens it in the default PDF
application on desktop or the share sheet on mobile. Unsupported formatting is
approximated and logged; documents that cannot be represented, such as those
with cells spanning multiple rows, fail to export.

## Updates

Desktop release builds check the latest GitHub release at startup and, if the
user agrees, install a newer signed version and restart.

To release, push a tag of `v` followed by the version in `tauri.conf.json`, then
publish the drafted release. Updater packages are signed with a key pair created
once with:

```sh
npx tauri signer generate -w ~/.tauri/verditum.key
```

The public key is `plugins.updater.pubkey` in `tauri.conf.json`. Store the
private key's contents in the `TAURI_SIGNING_PRIVATE_KEY` repository secret and
its password, if any, in `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`.

## License

Verditum is licensed under the [Apache License 2.0](LICENSE).

Run `npm run license-header` to refresh source license headers.
