/*
 * Copyright (c) 1986-2026 Ecmel Ercan
 * See LICENSE in the project root for license information.
 */

use crate::signature::{self, Report, Status, MAX_SIGNATURE_BYTES};
use std::io::{Read, Seek, SeekFrom};
use zip::ZipArchive;

const MAX_CONTENT_BYTES: usize = 32 * 1024 * 1024;

pub struct Document {
  pub content: String,
  pub signature: Report,
}

/// Verify original bytes before converting XML to text. Duplicate protected
/// entries are rejected so the renderer and verifier cannot select different data.
pub fn read(reader: impl Read + Seek) -> Result<Document, &'static str> {
  let archive = ZipArchive::new(reader).map_err(|_| "UDF arşivi okunamadı.")?;
  let directory_start = archive.central_directory_start();
  let unique_entries = archive.len();
  let mut reader = archive.into_inner();
  // zip 2 indexes entries by name and silently collapses duplicates. Compare
  // the raw central-directory count with that index before trusting it.
  let entry_count = central_entry_count(&mut reader, directory_start)
    .map_err(|_| "UDF arşiv dizini okunamadı veya çok fazla kayıt içeriyor.")?;
  if entry_count != unique_entries {
    return Err("UDF arşivi yinelenmiş dosya kayıtları içeriyor.");
  }
  let mut archive =
    ZipArchive::new(reader).map_err(|_| "UDF arşivi okunamadı.")?;
  let mut contents = 0;
  let mut signatures = 0;
  for index in 0..archive.len() {
    let entry = archive
      .by_index_raw(index)
      .map_err(|_| "UDF arşivi okunamadı.")?;
    match entry.name() {
      "content.xml" => contents += 1,
      "sign.sgn" => signatures += 1,
      _ => {}
    }
  }
  if contents != 1 || signatures > 1 {
    return Err(
      "UDF arşivindeki belge veya imza kayıtları eksik ya da yinelenmiş.",
    );
  }
  let content = read_entry(&mut archive, "content.xml", MAX_CONTENT_BYTES)
    .map_err(|_| "Belge okunamadı veya 32 MB sınırını aşıyor.")?;
  let signature = if signatures == 0 {
    Report::new(Status::Unsigned)
  } else {
    match read_entry(&mut archive, "sign.sgn", MAX_SIGNATURE_BYTES) {
      Ok(bytes) => signature::verify(&content, Some(&bytes)),
      Err(status) => Report::new(status),
    }
  };
  let content = String::from_utf8(content)
    .map_err(|_| "Belge geçerli UTF-8 metni içermiyor.")?;
  Ok(Document { content, signature })
}

fn central_entry_count(
  reader: &mut (impl Read + Seek),
  start: u64,
) -> std::io::Result<usize> {
  reader.seek(SeekFrom::Start(start))?;
  for count in 0..=4096 {
    let mut magic = [0; 4];
    reader.read_exact(&mut magic)?;
    match &magic {
      b"PK\x05\x06" | b"PK\x06\x06" => return Ok(count),
      b"PK\x01\x02" if count < 4096 => {
        let mut header = [0; 42];
        reader.read_exact(&mut header)?;
        let extra: u64 = [24, 26, 28]
          .into_iter()
          .map(|offset| {
            u16::from_le_bytes([header[offset], header[offset + 1]]) as u64
          })
          .sum();
        reader.seek(SeekFrom::Current(extra as i64))?;
      }
      _ => break,
    }
  }
  Err(std::io::Error::new(
    std::io::ErrorKind::InvalidData,
    "invalid ZIP directory",
  ))
}

fn read_entry<R: Read + Seek>(
  archive: &mut ZipArchive<R>,
  name: &str,
  limit: usize,
) -> Result<Vec<u8>, Status> {
  let entry = archive.by_name(name).map_err(|_| Status::Invalid)?;
  if entry.size() > limit as u64 {
    return Err(Status::Unsupported);
  }
  let mut bytes = Vec::new();
  entry
    .take(limit as u64 + 1)
    .read_to_end(&mut bytes)
    .map_err(|_| Status::Invalid)?;
  if bytes.len() > limit {
    return Err(Status::Unsupported);
  }
  Ok(bytes)
}

#[cfg(test)]
mod tests {
  use super::*;
  use std::io::{Cursor, Write};
  use zip::write::SimpleFileOptions;

  const CONTENT: &[u8] = include_bytes!("fixtures/signatures/content.xml");
  const SIGNATURE: &[u8] = include_bytes!("fixtures/signatures/rsa.der");

  fn archive(entries: &[(&str, &[u8])]) -> Vec<u8> {
    let mut archive = zip::ZipWriter::new(Cursor::new(Vec::new()));
    for (name, bytes) in entries {
      archive
        .start_file(*name, SimpleFileOptions::default())
        .unwrap();
      archive.write_all(bytes).unwrap();
    }
    archive.finish().unwrap().into_inner()
  }

  #[test]
  fn verifies_the_bytes_used_by_the_viewer() {
    let bytes = archive(&[("content.xml", CONTENT), ("sign.sgn", SIGNATURE)]);
    let document = read(Cursor::new(bytes)).unwrap();
    assert_eq!(document.signature.status, Status::Verified);
    assert_eq!(document.content.as_bytes(), CONTENT);
    let bytes =
      archive(&[("content.xml", b"<template/>"), ("sign.sgn", SIGNATURE)]);
    assert_eq!(
      read(Cursor::new(bytes)).unwrap().signature.status,
      Status::Invalid
    );
  }

  #[test]
  fn absent_and_broken_signatures_still_allow_reading_but_never_claim_verification(
  ) {
    for (signature, expected) in [
      (None, Status::Unsigned),
      (Some(b"broken".as_slice()), Status::Invalid),
    ] {
      let mut entries = vec![("content.xml", CONTENT)];
      if let Some(bytes) = signature {
        entries.push(("sign.sgn", bytes));
      }
      let document = read(Cursor::new(archive(&entries))).unwrap();
      assert_eq!(document.signature.status, expected);
    }
  }

  #[test]
  fn rejects_duplicate_protected_entries() {
    for (name, alternate) in
      [("content.xml", "contenX.xml"), ("sign.sgn", "sigX.sgn")]
    {
      let mut entries = vec![("content.xml", CONTENT), ("sign.sgn", SIGNATURE)];
      entries.push((alternate, b"ambiguous"));
      let mut bytes = archive(&entries);
      // ZIP writers forbid duplicates; rename the extra entry in both headers.
      for offset in 0..bytes.len() - alternate.len() {
        if &bytes[offset..offset + alternate.len()] == alternate.as_bytes() {
          bytes[offset..offset + alternate.len()]
            .copy_from_slice(name.as_bytes());
        }
      }
      assert!(read(Cursor::new(bytes)).is_err());
    }
  }

  #[test]
  fn rejects_bad_archives_missing_content_and_non_utf8() {
    assert!(read(Cursor::new(b"broken")).is_err());
    assert!(read(Cursor::new(archive(&[("sign.sgn", SIGNATURE)]))).is_err());
    assert!(read(Cursor::new(archive(&[("content.xml", &[0xff])]))).is_err());
  }

  #[test]
  fn bounds_decompressed_entry_sizes() {
    let oversized = vec![b' '; MAX_CONTENT_BYTES + 1];
    assert!(read(Cursor::new(archive(&[("content.xml", &oversized)]))).is_err());
    let oversized = vec![0; MAX_SIGNATURE_BYTES + 1];
    let bytes = archive(&[("content.xml", CONTENT), ("sign.sgn", &oversized)]);
    assert_eq!(
      read(Cursor::new(bytes)).unwrap().signature.status,
      Status::Unsupported
    );
  }

  /// Local-only compatibility check. Documents are read in place, never copied
  /// into the repository or printed. Normal test runs use synthetic fixtures.
  #[test]
  #[ignore = "requires VERDITUM_SIGNATURE_SAMPLES pointing to local signed UDF files"]
  fn local_signed_samples() {
    let directory = std::env::var("VERDITUM_SIGNATURE_SAMPLES").unwrap();
    let mut checked = 0;
    for entry in std::fs::read_dir(directory).unwrap() {
      let path = entry.unwrap().path();
      if path
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("udf"))
      {
        let document = read(std::fs::File::open(&path).unwrap()).unwrap();
        assert_eq!(document.signature.status, Status::Verified);
        checked += 1;
      }
    }
    assert!(checked > 0, "no UDF samples found");
    println!(
      "Verified {checked} local documents without certificate trust checks."
    );
  }
}
