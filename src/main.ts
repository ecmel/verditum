/*
 * Copyright (c) 1986-2026 Ecmel Ercan
 * See LICENSE in the project root for license information.
 */

import { css, html, LitElement, PropertyValues, unsafeCSS } from "lit";
import { customElement, query, state } from "lit/decorators.js";
import { keyed } from "lit/directives/keyed.js";
import { unsafeSVG } from "lit/directives/unsafe-svg.js";
import animations from "open-props/shadow/animations?inline";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { info, warn, error } from "@tauri-apps/plugin-log";
import { renderUdf, type RenderedDocument } from "./udf/render";
import { documentStyles } from "./udf/styles";
import {
  signatureLabels,
  signatureDescriptions,
  type SignatureReport,
} from "./signature";
import bi_sun from "./assets/sun.svg?raw";
import bi_moon from "./assets/moon.svg?raw";
import bi_folder2_open from "./assets/folder2-open.svg?raw";
import bi_zoom_in from "./assets/zoom-in.svg?raw";
import bi_zoom_out from "./assets/zoom-out.svg?raw";
import bi_share from "./assets/share.svg?raw";
import bi_arrows_expand_vertical from "./assets/arrows-expand-vertical.svg?raw";
import bi_chevron_right from "./assets/chevron-right.svg?raw";

/** Zoom limits in 10% steps from 100%: 30% to 300%. */
const MIN_ZOOM = -7;
const MAX_ZOOM = 20;

type Theme = "light" | "dark";

@customElement("main-view")
export class MainView extends LitElement {
  static styles = [
    // Keyframes must live in the shadow root to be found by its elements.
    unsafeCSS(animations),
    documentStyles,
    css`
      :host {
        --control-size: 2.5rem;
        position: relative;
        display: flex;
        flex-direction: column;
        box-sizing: border-box;
        height: 100dvh;
      }

      .menu {
        display: flex;
        justify-content: space-between;
        position: absolute;
        top: 0;
        left: 0;
        right: 0;
        z-index: 100;
        padding-top: var(--size-1);
        pointer-events: none;
      }

      .viewport {
        flex: 1;
        min-height: 0;
        overflow: auto;
        /* Start the document below the menu, which floats over the viewport. */
        padding-top: calc(var(--size-1) + var(--control-size));
      }

      .group {
        display: inline-flex;
        pointer-events: auto;
      }

      button,
      label {
        position: relative;
        display: inline-flex;
        align-items: center;
        justify-content: center;
        box-sizing: border-box;
        min-width: var(--control-size);
        height: var(--control-size);
        margin: 0;
        padding: 0 var(--size-3);
        border: var(--border-size-1) solid var(--border);
        border-radius: 0;
        background: var(--surface-raised);
        color: var(--text);
        font: inherit;
        cursor: pointer;
        transition:
          background-color 0.15s var(--ease-2),
          border-color 0.15s var(--ease-2);
      }

      button svg,
      label svg {
        width: 1em;
        height: 1em;
      }

      .group > * + * {
        margin-inline-start: calc(-1 * var(--border-size-1));
      }

      .group > :first-child {
        border-start-start-radius: var(--radius-2);
        border-end-start-radius: var(--radius-2);
      }

      .group > :last-child {
        border-start-end-radius: var(--radius-2);
        border-end-end-radius: var(--radius-2);
      }

      .primary,
      [aria-pressed="true"],
      label:has(:checked) {
        z-index: 1;
        border-color: var(--accent);
        background: var(--accent);
        color: var(--on-accent);
      }

      @media (hover: hover) {
        button:enabled:hover,
        label:hover {
          background: color-mix(
            in srgb,
            var(--accent) 12%,
            var(--surface-raised)
          );
        }

        .primary:enabled:hover,
        [aria-pressed="true"]:enabled:hover,
        label:has(:checked):hover {
          border-color: var(--accent-hover);
          background: var(--accent-hover);
        }
      }

      button:focus-visible,
      label:has(:focus-visible) {
        z-index: 2;
        outline: var(--border-size-2) solid var(--accent);
        outline-offset: var(--border-size-2);
      }

      button:disabled {
        opacity: 0.5;
        cursor: not-allowed;
      }

      button[aria-busy="true"] {
        cursor: wait;
      }

      button[aria-busy="true"] svg {
        visibility: hidden;
      }

      button[aria-busy="true"]::after {
        content: "";
        position: absolute;
        inset: 0;
        width: 1em;
        height: 1em;
        margin: auto;
        box-sizing: border-box;
        border: var(--border-size-2) solid currentColor;
        border-inline-end-color: transparent;
        border-radius: var(--radius-round);
        animation: var(--animation-spin);
        animation-duration: 1s;
      }

      /* The radio covers its label, so clicks and focus reach it natively. */
      label input {
        position: absolute;
        inset: 0;
        margin: 0;
        opacity: 0;
        cursor: inherit;
      }

      /* Open Props names slides by the direction they travel. */
      @media (prefers-reduced-motion: no-preference) {
        .slide-in-right {
          animation: var(--animation-slide-in-right);
          animation-duration: 250ms;
        }

        .slide-in-left {
          animation: var(--animation-slide-in-left);
          animation-duration: 250ms;
        }
      }

      .signature-panel {
        margin: var(--size-2);
        padding: 0.75rem var(--size-3);
        border-top: var(--border-size-1) solid var(--border);
        color: var(--text);
        font-size: 0.875rem;
        line-height: var(--font-lineheight-3);
        overflow-wrap: anywhere;
      }

      .signature-panel[data-status="invalid"] {
        border-color: var(--danger);
      }

      .signature-panel summary {
        display: flex;
        align-items: center;
        gap: var(--size-2);
        list-style: none;
        cursor: pointer;
      }

      /* WebKit keeps its marker unless it is hidden explicitly. */
      .signature-panel summary::-webkit-details-marker {
        display: none;
      }

      .signature-panel summary svg {
        flex: none;
        width: 0.75em;
        height: 0.75em;
        color: var(--text-muted);
        transition: rotate 0.2s var(--ease-3);
      }

      .signature-panel details[open] > summary svg {
        rotate: 90deg;
      }

      .signature-panel summary:focus-visible {
        outline: var(--border-size-2) solid var(--accent);
        outline-offset: 4px;
      }

      .signature-panel .trust-note {
        margin: 0.35rem 0 0;
        color: var(--text-muted);
      }

      .signature-panel li + li {
        margin-top: 0.75rem;
      }

      @media print {
        :host {
          display: block;
          height: auto;
        }

        .viewport {
          overflow: visible;
          padding-top: 0;
        }

        .menu,
        .signature-panel {
          display: none;
        }
      }
    `,
  ];

  @state()
  protected zoom = 0;

  @state()
  protected fitWidth = true;

  private resizeObserver = new ResizeObserver(() => this.updateFitWidth());

  @state()
  protected document?: RenderedDocument;

  @state()
  protected exporting = false;

  /** Source XML of the displayed document, kept for PDF export. */
  private content = "";

  @state()
  protected documentError = "";

  @state()
  protected signature?: SignatureReport;

  @state()
  protected theme: Theme = "light";

  @query(".viewport")
  protected viewport!: HTMLDivElement;

  constructor() {
    super();
    const theme = localStorage.getItem("theme");
    if (theme === "light" || theme === "dark") {
      this.theme = theme;
    }
    const zoom = localStorage.getItem("zoom");
    if (zoom) {
      const value = Number(zoom);
      this.zoom = Number.isFinite(value)
        ? Math.max(MIN_ZOOM, Math.min(MAX_ZOOM, value))
        : 0;
    }
  }

  connectedCallback(): void {
    super.connectedCallback();
    this.resizeObserver.observe(this);
    if (this.viewport) this.resizeObserver.observe(this.viewport);
  }

  disconnectedCallback(): void {
    this.resizeObserver.disconnect();
    super.disconnectedCallback();
  }

  private updateFitWidth() {
    const page = this.document?.element;
    if (
      !this.fitWidth ||
      !page?.isConnected ||
      window.matchMedia("print").matches
    )
      return;
    const pageWidth = parseFloat(getComputedStyle(page).width);
    const viewportWidth = this.viewport.clientWidth;
    if (pageWidth > 0 && viewportWidth > 0) {
      // Leave one pixel for fractional layout rounding to avoid horizontal overflow.
      const zoom = ((viewportWidth - 1) / pageWidth - 1) * 10;
      if (Math.abs(this.zoom - zoom) > 0.001) this.zoom = zoom;
    }
  }

  private changeZoom(step: number) {
    this.fitWidth = false;
    // Fitting a narrow window can go below the minimum; zooming out must not
    // enlarge the page from there.
    const min = Math.min(MIN_ZOOM, this.zoom);
    this.zoom = Math.max(min, Math.min(MAX_ZOOM, this.zoom + step));
  }

  protected toTop() {
    this.viewport.scrollTop = 0;
  }

  protected addContent(payload: OpenPayload) {
    try {
      const rendered = renderUdf(payload.content);
      this.document = rendered;
      this.content = payload.content;
      this.documentError = "";
      this.signature = payload.signature ?? {
        status: "unsupported",
        signers: [],
      };
      let printStyle =
        document.querySelector<HTMLStyleElement>("#udf-print-style");
      if (!printStyle) {
        printStyle = document.createElement("style");
        printStyle.id = "udf-print-style";
        document.head.append(printStyle);
      }
      printStyle.textContent = rendered.printStyle;
      this.toTop();
    } catch (err) {
      this.clearDocument(
        err instanceof Error ? err.message : "Unable to display this document.",
      );
    }
  }

  private clearDocument(message: string) {
    this.document = undefined;
    this.signature = undefined;
    this.content = "";
    document.querySelector("#udf-print-style")?.remove();
    this.documentError = message;
  }

  protected async exportPdf() {
    if (!this.content || this.exporting) return;
    this.exporting = true;
    try {
      const warnings = await invoke<string[]>("export", {
        content: this.content,
      });
      // The PDF opening is feedback enough; the rest goes to the log.
      if (warnings?.length) warn(`export: ${warnings.join(" ")}`);
    } catch (err) {
      error(`export: ${err}`);
    } finally {
      this.exporting = false;
    }
  }

  protected updated(changedProperties: PropertyValues): void {
    if (changedProperties.has("theme")) {
      document.documentElement.dataset.theme = this.theme;
      localStorage.setItem("theme", this.theme);
    }
    if (changedProperties.has("zoom") && !this.fitWidth) {
      localStorage.setItem("zoom", this.zoom.toString());
    }
    if (
      changedProperties.has("document") ||
      changedProperties.has("fitWidth")
    ) {
      this.updateFitWidth();
    }
  }

  protected firstUpdated(_changedProperties: PropertyValues): void {
    this.resizeObserver.observe(this.viewport);
    Promise.all([
      listen<OpenPayload>("add-content", (e) => this.addContent(e.payload)),
      listen<string>("open-error", (e) => this.clearDocument(e.payload)),
    ])
      .then(() => invoke("ready"))
      .then(() => info("Ready"))
      .catch((err) => error(`${err}`));
  }

  protected render() {
    return html`
      <div class="menu">
        <div class="group slide-in-right">
          <button
            class="primary"
            aria-label="Open document"
            title="Open document"
            @click="${() => invoke("pick")}"
          >
            ${unsafeSVG(bi_folder2_open)}
          </button>
          <button
            aria-label="Export PDF"
            title="Export PDF"
            aria-busy=${this.exporting ? "true" : "false"}
            ?disabled=${!this.document}
            @click=${() => this.exportPdf()}
          >
            ${unsafeSVG(bi_share)}
          </button>
          <button aria-label="Zoom out" @click="${() => this.changeZoom(-1)}">
            ${unsafeSVG(bi_zoom_out)}
          </button>
          <button aria-label="Zoom in" @click="${() => this.changeZoom(1)}">
            ${unsafeSVG(bi_zoom_in)}
          </button>
          <button
            aria-label="Fit page width"
            title="Fit page width"
            aria-pressed=${this.fitWidth ? "true" : "false"}
            ?disabled=${!this.document}
            @click=${() => {
              this.fitWidth = !this.fitWidth;
            }}
          >
            ${unsafeSVG(bi_arrows_expand_vertical)}
          </button>
        </div>
        <div
          class="group slide-in-left"
          role="radiogroup"
          aria-label="Theme"
          @change="${(e: Event) => {
            this.theme = (e.target as HTMLInputElement).value as Theme;
          }}"
        >
          <label title="Dark theme">
            <input
              type="radio"
              name="theme"
              value="dark"
              aria-label="Dark theme"
              .checked=${this.theme === "dark"}
            />
            ${unsafeSVG(bi_moon)}
          </label>
          <label title="Light theme">
            <input
              type="radio"
              name="theme"
              value="light"
              aria-label="Light theme"
              .checked=${this.theme === "light"}
            />
            ${unsafeSVG(bi_sun)}
          </label>
        </div>
      </div>
      <div class="viewport">
        ${keyed(
          this.document,
          html`<div class="slide-in-left">
            ${this.documentError ? html`<p class="document-notice" role="alert">${this.documentError}</p>` : ""}
            <div
              class="content"
              data-theme=${this.theme}
              style="zoom: ${1 + this.zoom * 0.1};"
            >
              ${this.document?.element}
            </div>
          </div>`,
        )}
        ${this.renderSignature()}
      </div>
    `;
  }

  private renderSignature() {
    const report = this.signature;
    if (!report) return "";
    return html`
      <section
        class="signature-panel"
        lang="tr"
        data-status=${report.status}
        aria-label="Elektronik imza bilgileri"
      >
        <details>
          <summary>
            ${unsafeSVG(bi_chevron_right)}
            <span role=${report.status === "invalid" ? "alert" : "status"}>
              ${signatureLabels[report.status]}
            </span>
          </summary>
          <p>${signatureDescriptions[report.status]}</p>
          ${
            report.signers.length
              ? html`<ul>
                  ${report.signers.map(
                    (signer) =>
                      html`<li>
                        <strong>${signatureLabels[signer.status]}</strong><br />
                        Sertifikada yazan kimlik:
                        ${signer.subject || "Sertifika bulunamadı"}<br />
                        Sertifikayı düzenleyen: ${signer.issuer || "Bilinmiyor"}
                      </li>`,
                  )}
                </ul>`
              : ""
          }
          ${
            report.status !== "unsigned"
              ? html`<p>
                    Sertifika zinciri, geçerlilik süresi, iptal durumu, zaman
                    damgaları ve karşı imzalar kontrol edilmedi. Bu sonuç,
                    imzalayanın kimliğinin güvenilir olduğunu onaylamaz.
                  </p>
                  <p>PDF çıktısı özgün UDF elektronik imzasını taşımaz.</p>`
              : ""
          }
        </details>
        ${
          report.status !== "unsigned"
            ? html`<p class="trust-note">Sertifika güveni kontrol edilmedi.</p>`
            : ""
        }
      </section>
    `;
  }
}

type OpenPayload = {
  content: string;
  size: number;
  filePath: string;
  signature?: SignatureReport;
};

declare global {
  interface HTMLElementTagNameMap {
    "main-view": MainView;
  }
}
