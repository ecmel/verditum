#!/usr/bin/env bash
# Regenerate synthetic CMS test fixtures. Requires OpenSSL 3; no real documents
# or persistent private keys are used. Run from the repository root.
set -euo pipefail
fixture_dir="src-tauri/src/fixtures/signatures"
fixture_tmp=$(mktemp -d)
trap 'rm -rf "$fixture_tmp"' EXIT
mkdir -p "$fixture_dir"
printf '%s\n' '<template><content>Verditum imza testi — örnek belge.</content></template>' > "$fixture_dir/content.xml"
for kind in rsa p256 p384; do
  case "$kind" in
    rsa) key_args=(-newkey rsa:2048) ;;
    p256) key_args=(-newkey ec -pkeyopt ec_paramgen_curve:P-256) ;;
    p384) key_args=(-newkey ec -pkeyopt ec_paramgen_curve:P-384) ;;
  esac
  openssl req -x509 "${key_args[@]}" -noenc -sha256 -days 3650 \
    -subj "/CN=Verditum Test $kind/O=Synthetic Test Only" \
    -keyout "$fixture_tmp/$kind.key" -out "$fixture_tmp/$kind.pem" 2>/dev/null
done
sign() {
  local name="$1" kind="$2" digest="$3"
  shift 3
  openssl cms -sign -binary -in "$fixture_dir/content.xml" \
    -signer "$fixture_tmp/$kind.pem" -inkey "$fixture_tmp/$kind.key" \
    -md "$digest" -outform DER -out "$fixture_dir/$name.der" "$@"
}
sign rsa rsa sha256
sign p256 p256 sha256
sign p384 p384 sha384
sign no-attributes rsa sha512 -noattr
sign key-id p256 sha256 -keyid
sign multiple rsa sha256 -signer "$fixture_tmp/p256.pem" -inkey "$fixture_tmp/p256.key"
sign sha1 rsa sha1
sign rsa-pss rsa sha256 -keyopt rsa_padding_mode:pss
