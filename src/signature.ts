/*
 * Copyright (c) 1986-2026 Ecmel Ercan
 * See LICENSE in the project root for license information.
 */

export type SignatureStatus =
  "unsigned" | "verified" | "invalid" | "unsupported";

export type SignatureReport = {
  status: SignatureStatus;
  signers: {
    subject: string;
    issuer: string;
    status: SignatureStatus;
  }[];
};

export const signatureLabels: Record<SignatureStatus, string> = {
  unsigned: "Elektronik imza yok",
  verified: "İmza bütünlüğü doğrulandı",
  invalid: "İmza doğrulanamadı",
  unsupported: "İmza denetlenemedi",
};

export const signatureDescriptions: Record<SignatureStatus, string> = {
  unsigned: "Bu UDF dosyasında elektronik imza kaydı bulunamadı.",
  verified:
    "Tüm ana imzalar belgenin özgün içeriğiyle eşleşiyor. Dosyadaki diğer kayıtlar bu kontrolün kapsamında değildir.",
  invalid:
    "Belge imzayla eşleşmiyor veya imza kaydı eksik, bozuk ya da tutarsız. Belgenin bütünlüğü doğrulanamadı.",
  unsupported:
    "İmza biçimi, algoritması veya boyutu desteklenmediği için kontrol tamamlanamadı.",
};
