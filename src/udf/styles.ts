/*
 * Copyright (c) 1986-2026 Ecmel Ercan
 * See LICENSE in the project root for license information.
 */

import { css } from "lit";

export const documentStyles = css`
  .content {
    padding: 12px 0;
  }
  .udf-document {
    box-sizing: border-box;
    margin: 0 auto;
    background: white;
    color: black;
    font:
      12pt "Times New Roman",
      serif;
    white-space: pre-wrap;
    overflow-wrap: anywhere;
    tab-size: 4;
  }
  .udf-document p {
    margin: 0;
    min-height: 1em;
  }
  .udf-document table {
    border-collapse: collapse;
    table-layout: fixed;
  }
  .udf-document td {
    padding: 0 5.4pt;
  }
  .udf-document img {
    max-width: 100%;
    object-fit: contain;
  }
  .udf-document header {
    margin-bottom: 12pt;
  }
  .udf-document footer {
    margin-top: 12pt;
  }
  .udf-page-break {
    break-before: page;
    border-top: 1px dashed #aaa;
    margin-top: 24pt;
    padding-top: 24pt;
  }
  .document-notice {
    margin: 8px 0;
    font-size: 0.9rem;
  }
  @media screen {
    .content[data-theme="dark"] .udf-document,
    .content[data-theme="dark"] .udf-document img {
      filter: invert(1) hue-rotate(180deg);
    }
  }
  @media print {
    .content {
      overflow: visible;
      padding: 0;
      zoom: 1 !important;
    }
    .udf-document {
      width: auto !important;
      min-height: 0 !important;
      padding: 0 !important;
      margin: 0;
    }
    .udf-page-break {
      border: 0;
      margin: 0;
      padding: 0;
    }
    .document-notice {
      display: none;
    }
  }
`;
