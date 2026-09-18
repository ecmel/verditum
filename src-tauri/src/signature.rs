/*
 * Copyright (c) 1986-2026 Ecmel Ercan
 * See LICENSE in the project root for license information.
 */

//! Offline CMS integrity verification, NOT certificate trust validation.
//! Only the exact content.xml bytes are covered. Other ZIP entries, certificate
//! chains, validity periods, revocation, timestamps and countersignatures are
//! not validated. All top-level signers must verify for an aggregate success.

use cms::cert::CertificateChoices;
use cms::content_info::ContentInfo;
use cms::signed_data::{SignedData, SignerIdentifier, SignerInfo};
use der::asn1::{ObjectIdentifier, OctetString};
use der::{Decode, Encode};
use ring::{digest, signature};
use serde::Serialize;
use x509_cert::ext::pkix::SubjectKeyIdentifier;
use x509_cert::Certificate;

pub const MAX_SIGNATURE_BYTES: usize = 2 * 1024 * 1024;
const MAX_SIGNERS: usize = 16;
const DATA: &str = "1.2.840.113549.1.7.1";
const SIGNED_DATA: &str = "1.2.840.113549.1.7.2";
const CONTENT_TYPE: &str = "1.2.840.113549.1.9.3";
const MESSAGE_DIGEST: &str = "1.2.840.113549.1.9.4";
const SHA256: &str = "2.16.840.1.101.3.4.2.1";
const SHA384: &str = "2.16.840.1.101.3.4.2.2";
const SHA512: &str = "2.16.840.1.101.3.4.2.3";
const RSA: &str = "1.2.840.113549.1.1.1";
const EC: &str = "1.2.840.10045.2.1";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Status {
  Unsigned,
  Verified,
  Invalid,
  Unsupported,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Signer {
  pub subject: String,
  pub issuer: String,
  pub status: Status,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Report {
  pub status: Status,
  pub signers: Vec<Signer>,
}

impl Report {
  pub fn new(status: Status) -> Self {
    Self {
      status,
      signers: Vec::new(),
    }
  }
}

pub fn verify(content: &[u8], signature: Option<&[u8]>) -> Report {
  let Some(bytes) = signature else {
    return Report::new(Status::Unsigned);
  };
  if bytes.len() > MAX_SIGNATURE_BYTES {
    return Report::new(Status::Unsupported);
  }
  let envelope = match ContentInfo::from_der(bytes) {
    Ok(value) => value,
    Err(_) => return Report::new(Status::Invalid),
  };
  if envelope.content_type.to_string() != SIGNED_DATA {
    return Report::new(Status::Unsupported);
  }
  let data = match envelope.content.decode_as::<SignedData>() {
    Ok(value) => value,
    Err(_) => return Report::new(Status::Invalid),
  };
  if data.encap_content_info.econtent_type.to_string() != DATA
    || data.encap_content_info.econtent.is_some()
    || data.signer_infos.0.len() > MAX_SIGNERS
  {
    return Report::new(Status::Unsupported);
  }
  if data.signer_infos.0.is_empty() {
    return Report::new(Status::Invalid);
  }

  let mut report = Report::new(Status::Verified);
  for signer in data.signer_infos.0.iter() {
    let certificates: Vec<_> = data
      .certificates
      .iter()
      .flat_map(|set| set.0.iter())
      .filter_map(|choice| match choice {
        CertificateChoices::Certificate(cert)
          if matches_signer(cert, signer) =>
          Some(cert),
        _ => None,
      })
      .collect();
    // Never pick an arbitrary certificate when the signer identity is ambiguous.
    let (status, subject, issuer) = if certificates.len() == 1 {
      let cert = certificates[0];
      let status = verify_signer(content, &data, signer, cert)
        .err()
        .unwrap_or(Status::Verified);
      (
        status,
        cert.tbs_certificate.subject.to_string(),
        cert.tbs_certificate.issuer.to_string(),
      )
    } else {
      (Status::Invalid, String::new(), String::new())
    };
    report.signers.push(Signer {
      subject,
      issuer,
      status,
    });
    report.status = match (report.status, status) {
      (Status::Invalid, _) | (_, Status::Invalid) => Status::Invalid,
      (Status::Unsupported, _) | (_, Status::Unsupported) =>
        Status::Unsupported,
      _ => Status::Verified,
    };
  }
  report
}

fn matches_signer(cert: &Certificate, signer: &SignerInfo) -> bool {
  match &signer.sid {
    SignerIdentifier::IssuerAndSerialNumber(id) =>
      cert.tbs_certificate.issuer == id.issuer
        && cert.tbs_certificate.serial_number == id.serial_number,
    SignerIdentifier::SubjectKeyIdentifier(id) => cert
      .tbs_certificate
      .get::<SubjectKeyIdentifier>()
      .ok()
      .flatten()
      .is_some_and(|(_, key_id)| key_id == *id),
  }
}

fn verify_signer(
  content: &[u8],
  data: &SignedData,
  signer: &SignerInfo,
  cert: &Certificate,
) -> Result<(), Status> {
  let digest_oid = signer.digest_alg.oid.to_string();
  let hash = match digest_oid.as_str() {
    SHA256 => &digest::SHA256,
    SHA384 => &digest::SHA384,
    SHA512 => &digest::SHA512,
    _ => return Err(Status::Unsupported),
  };
  if !data.digest_algorithms.iter().any(|alg| {
    alg.oid == signer.digest_alg.oid && null_or_absent(&alg.parameters)
  }) {
    return Err(Status::Invalid);
  }
  if !null_or_absent(&signer.digest_alg.parameters) {
    return Err(Status::Unsupported);
  }

  let signed_bytes;
  let message = if let Some(attrs) = &signer.signed_attrs {
    // Both attributes must occur exactly once, with exactly one value (RFC 5652).
    let attribute = |oid: &str| -> Result<&der::Any, Status> {
      let mut matches = attrs.iter().filter(|attr| attr.oid.to_string() == oid);
      let attr = matches.next().ok_or(Status::Invalid)?;
      if matches.next().is_some() || attr.values.len() != 1 {
        return Err(Status::Invalid);
      }
      attr.values.get(0).ok_or(Status::Invalid)
    };
    let content_type = attribute(CONTENT_TYPE)?
      .decode_as::<ObjectIdentifier>()
      .map_err(|_| Status::Invalid)?;
    if content_type != data.encap_content_info.econtent_type {
      return Err(Status::Invalid);
    }
    let expected = attribute(MESSAGE_DIGEST)?
      .decode_as::<OctetString>()
      .map_err(|_| Status::Invalid)?;
    if expected.as_bytes() != digest::digest(hash, content).as_ref() {
      return Err(Status::Invalid);
    }
    // Signed attributes are signed as a DER SET, not the implicit [0] tag.
    signed_bytes = attrs.to_der().map_err(|_| Status::Invalid)?;
    signed_bytes.as_slice()
  } else {
    content
  };

  let key = &cert.tbs_certificate.subject_public_key_info;
  let key_oid = key.algorithm.oid.to_string();
  let signature_oid = signer.signature_algorithm.oid.to_string();
  let algorithm: &dyn signature::VerificationAlgorithm = match key_oid.as_str()
  {
    RSA => {
      if !null_or_absent(&key.algorithm.parameters)
        || !null_or_absent(&signer.signature_algorithm.parameters)
      {
        return Err(Status::Unsupported);
      }
      match (digest_oid.as_str(), signature_oid.as_str()) {
        (SHA256, RSA | "1.2.840.113549.1.1.11") =>
          &signature::RSA_PKCS1_2048_8192_SHA256,
        (SHA384, RSA | "1.2.840.113549.1.1.12") =>
          &signature::RSA_PKCS1_2048_8192_SHA384,
        (SHA512, RSA | "1.2.840.113549.1.1.13") =>
          &signature::RSA_PKCS1_2048_8192_SHA512,
        _ => return Err(Status::Unsupported),
      }
    }
    EC => {
      if signer.signature_algorithm.parameters.is_some() {
        return Err(Status::Unsupported);
      }
      let curve = key
        .algorithm
        .parameters
        .as_ref()
        .and_then(|value| value.decode_as::<ObjectIdentifier>().ok())
        .ok_or(Status::Unsupported)?
        .to_string();
      match (curve.as_str(), digest_oid.as_str(), signature_oid.as_str()) {
        ("1.2.840.10045.3.1.7", SHA256, "1.2.840.10045.4.3.2") =>
          &signature::ECDSA_P256_SHA256_ASN1,
        ("1.2.840.10045.3.1.7", SHA384, "1.2.840.10045.4.3.3") =>
          &signature::ECDSA_P256_SHA384_ASN1,
        ("1.3.132.0.34", SHA256, "1.2.840.10045.4.3.2") =>
          &signature::ECDSA_P384_SHA256_ASN1,
        ("1.3.132.0.34", SHA384, "1.2.840.10045.4.3.3") =>
          &signature::ECDSA_P384_SHA384_ASN1,
        _ => return Err(Status::Unsupported),
      }
    }
    _ => return Err(Status::Unsupported),
  };
  let public_key = key.subject_public_key.as_bytes().ok_or(Status::Invalid)?;
  signature::UnparsedPublicKey::new(algorithm, public_key)
    .verify(message, signer.signature.as_bytes())
    .map_err(|_| Status::Invalid)
}

fn null_or_absent(parameters: &Option<der::Any>) -> bool {
  parameters
    .as_ref()
    .is_none_or(|value| value.decode_as::<()>().is_ok())
}

#[cfg(test)]
mod tests {
  use super::*;
  use der::asn1::SetOfVec;

  const CONTENT: &[u8] = include_bytes!("fixtures/signatures/content.xml");
  const RSA_SIGNATURE: &[u8] = include_bytes!("fixtures/signatures/rsa.der");
  const MULTIPLE: &[u8] = include_bytes!("fixtures/signatures/multiple.der");

  fn changed(bytes: &[u8], change: impl FnOnce(&mut SignedData)) -> Vec<u8> {
    let mut envelope = ContentInfo::from_der(bytes).unwrap();
    let mut data = envelope.content.decode_as::<SignedData>().unwrap();
    change(&mut data);
    envelope.content = der::Any::encode_from(&data).unwrap();
    envelope.to_der().unwrap()
  }

  fn change_first(data: &mut SignedData, change: impl FnOnce(&mut SignerInfo)) {
    let mut signers: Vec<_> = data.signer_infos.0.iter().cloned().collect();
    change(&mut signers[0]);
    data.signer_infos.0 = SetOfVec::try_from(signers).unwrap();
  }

  #[test]
  fn verifies_supported_signatures_without_trusting_self_signed_certificates() {
    for bytes in [
      RSA_SIGNATURE,
      include_bytes!("fixtures/signatures/p256.der"),
      include_bytes!("fixtures/signatures/p384.der"),
      include_bytes!("fixtures/signatures/no-attributes.der"),
      include_bytes!("fixtures/signatures/key-id.der"),
      MULTIPLE,
    ] {
      let report = verify(CONTENT, Some(bytes));
      assert_eq!(report.status, Status::Verified);
      assert!(report.signers.iter().all(|signer| {
        signer.status == Status::Verified
          && signer.subject.contains("Verditum Test")
      }));
      // Even a newline changes the exact byte sequence that was signed.
      let mut modified = CONTENT.to_vec();
      modified.push(b'\n');
      assert_eq!(verify(&modified, Some(bytes)).status, Status::Invalid);
    }
    assert_eq!(verify(CONTENT, Some(MULTIPLE)).signers.len(), 2);
  }

  #[test]
  fn verifies_signature_not_just_the_content_digest_and_requires_every_signer()
  {
    let bytes = changed(MULTIPLE, |data| {
      change_first(data, |signer| {
        let mut signature = signer.signature.as_bytes().to_vec();
        signature[0] ^= 1;
        signer.signature = OctetString::new(signature).unwrap();
      })
    });
    let report = verify(CONTENT, Some(&bytes));
    assert_eq!(report.status, Status::Invalid);
    assert_eq!(
      report
        .signers
        .iter()
        .filter(|s| s.status == Status::Verified)
        .count(),
      1
    );
  }

  #[test]
  fn rejects_missing_signers_and_missing_or_mismatched_certificates() {
    for bytes in [
      changed(RSA_SIGNATURE, |data| data.signer_infos.0 = SetOfVec::new()),
      changed(RSA_SIGNATURE, |data| data.certificates = None),
      changed(RSA_SIGNATURE, |data| {
        change_first(data, |signer| {
          if let SignerIdentifier::IssuerAndSerialNumber(id) = &mut signer.sid {
            id.serial_number =
              x509_cert::serial_number::SerialNumber::from(1u8);
          }
        })
      }),
    ] {
      assert_eq!(verify(CONTENT, Some(&bytes)).status, Status::Invalid);
    }
  }

  #[test]
  fn accepts_equivalent_absent_and_null_digest_parameters() {
    for parameters in [None, Some(der::Any::encode_from(&()).unwrap())] {
      let bytes = changed(RSA_SIGNATURE, |data| {
        let mut algorithms: Vec<_> =
          data.digest_algorithms.iter().cloned().collect();
        algorithms[0].parameters = parameters;
        data.digest_algorithms = SetOfVec::try_from(algorithms).unwrap();
      });
      assert_eq!(verify(CONTENT, Some(&bytes)).status, Status::Verified);
    }
  }

  #[test]
  fn rejects_missing_duplicate_or_incorrect_signed_attributes() {
    for oid in [CONTENT_TYPE, MESSAGE_DIGEST] {
      let missing = changed(RSA_SIGNATURE, |data| {
        change_first(data, |signer| {
          let attrs: Vec<_> = signer
            .signed_attrs
            .as_ref()
            .unwrap()
            .iter()
            .filter(|attr| attr.oid.to_string() != oid)
            .cloned()
            .collect();
          signer.signed_attrs = Some(SetOfVec::try_from(attrs).unwrap());
        })
      });
      assert_eq!(verify(CONTENT, Some(&missing)).status, Status::Invalid);

      let duplicate = changed(RSA_SIGNATURE, |data| {
        change_first(data, |signer| {
          let attrs = signer.signed_attrs.as_mut().unwrap();
          let mut extra = attrs
            .iter()
            .find(|attr| attr.oid.to_string() == oid)
            .unwrap()
            .clone();
          extra.values = SetOfVec::try_from(vec![der::Any::encode_from(
            &OctetString::new(vec![0]).unwrap(),
          )
          .unwrap()])
          .unwrap();
          attrs.insert(extra).unwrap();
        })
      });
      assert_eq!(verify(CONTENT, Some(&duplicate)).status, Status::Invalid);
    }
    let incorrect_type = changed(RSA_SIGNATURE, |data| {
      change_first(data, |signer| {
        let mut attrs: Vec<_> = signer
          .signed_attrs
          .as_ref()
          .unwrap()
          .iter()
          .cloned()
          .collect();
        let attr = attrs
          .iter_mut()
          .find(|attr| attr.oid.to_string() == CONTENT_TYPE)
          .unwrap();
        attr.values = SetOfVec::try_from(vec![der::Any::encode_from(
          &ObjectIdentifier::new_unwrap(SIGNED_DATA),
        )
        .unwrap()])
        .unwrap();
        signer.signed_attrs = Some(SetOfVec::try_from(attrs).unwrap());
      })
    });
    assert_eq!(
      verify(CONTENT, Some(&incorrect_type)).status,
      Status::Invalid
    );
  }

  #[test]
  fn distinguishes_absent_malformed_and_unsupported_signatures() {
    assert_eq!(verify(CONTENT, None).status, Status::Unsigned);
    for bytes in [b"".as_slice(), b"not a signature", &RSA_SIGNATURE[..50]] {
      assert_eq!(verify(CONTENT, Some(bytes)).status, Status::Invalid);
    }
    for bytes in [
      include_bytes!("fixtures/signatures/sha1.der").as_slice(),
      include_bytes!("fixtures/signatures/rsa-pss.der"),
      &vec![0; MAX_SIGNATURE_BYTES + 1],
    ] {
      assert_eq!(verify(CONTENT, Some(bytes)).status, Status::Unsupported);
    }
    let embedded = changed(RSA_SIGNATURE, |data| {
      data.encap_content_info.econtent = Some(
        der::Any::encode_from(&OctetString::new(CONTENT).unwrap()).unwrap(),
      );
    });
    assert_eq!(verify(CONTENT, Some(&embedded)).status, Status::Unsupported);
    let mut trailing = RSA_SIGNATURE.to_vec();
    trailing.push(0);
    assert_eq!(verify(CONTENT, Some(&trailing)).status, Status::Invalid);
  }
}
