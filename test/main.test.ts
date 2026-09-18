import { invoke } from "@tauri-apps/api/core";
import { warn, error } from "@tauri-apps/plugin-log";
import { listen } from "@tauri-apps/api/event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { MainView } from "../src/main";
import type { SignatureReport } from "../src/signature";
import "../src/main";
import "../src/styles.css";
import formattedXml from "./fixtures/formatted.xml?raw";

let view: MainView;

function button(label: string) {
  return view.shadowRoot!.querySelector<HTMLButtonElement>(
    `button[aria-label="${label}"]`,
  )!;
}

function busy() {
  return button("Export PDF").getAttribute("aria-busy");
}

/** Finishes the decorative slide-ins so layout can be measured. */
function settle() {
  view.shadowRoot!.getAnimations().forEach((animation) => animation.finish());
}

/** Answers the `export` command while leaving other commands inert. */
function mockExport(reply: () => unknown) {
  vi.mocked(invoke).mockImplementation(async (command) =>
    command === "export" ? reply() : undefined,
  );
}

function flush() {
  return new Promise((resolve) => setTimeout(resolve, 10));
}

/** How many times the backend has been asked to export, ignoring other calls. */
function exportCalls() {
  return vi.mocked(invoke).mock.calls.filter(([name]) => name === "export")
    .length;
}

function viewport() {
  return view.shadowRoot!.querySelector<HTMLDivElement>(".viewport")!;
}

function scale() {
  return parseFloat(
    view.shadowRoot!.querySelector<HTMLElement>(".content")!.style.zoom,
  );
}

function pageWidth() {
  return view
    .shadowRoot!.querySelector<HTMLElement>(".udf-document")!
    .getBoundingClientRect().width;
}

beforeEach(async () => {
  localStorage.clear();
  vi.clearAllMocks();
  mockExport(() => undefined);
  view = document.createElement("main-view");
  view.style.width = "600px";
  document.body.append(view);
  await view.updateComplete;
  settle();
});

afterEach(() => {
  view.remove();
  document.querySelector("#udf-print-style")?.remove();
  localStorage.clear();
  document.documentElement.dataset.theme = "light";
});

async function openDocument(
  content = formattedXml,
  signature?: SignatureReport,
) {
  const subscription = vi
    .mocked(listen)
    .mock.calls.find(([name]) => name === "add-content");
  expect(subscription).toBeDefined();
  subscription![1]({
    event: "add-content",
    id: 1,
    payload: {
      content,
      size: content.length,
      filePath: "example.udf",
      signature,
    },
  });
  await view.updateComplete;
  settle();
}

describe("document viewer", () => {
  it("fits by default, keeps zoom when toggled off, and fits again when enabled", async () => {
    expect(button("Fit page width").disabled).toBe(true);
    await openDocument();
    expect(button("Fit page width").getAttribute("aria-pressed")).toBe("true");
    await expect
      .poll(() => Math.abs(pageWidth() - (viewport().clientWidth - 1)))
      .toBeLessThan(1);

    button("Fit page width").click();
    await view.updateComplete;
    expect(button("Fit page width").getAttribute("aria-pressed")).toBe("false");
    const width = pageWidth();
    view.style.width = "450px";
    await new Promise<void>((resolve) =>
      requestAnimationFrame(() => requestAnimationFrame(() => resolve())),
    );
    expect(pageWidth()).toBeCloseTo(width, 0);

    button("Fit page width").click();
    await view.updateComplete;
    await expect
      .poll(() => Math.abs(pageWidth() - (viewport().clientWidth - 1)))
      .toBeLessThan(1);
  });

  it("scrolls in both directions within the window after zooming in", async () => {
    await openDocument();
    await expect
      .poll(() => Math.abs(pageWidth() - (viewport().clientWidth - 1)))
      .toBeLessThan(1);
    for (let i = 0; i < 8; i++) button("Zoom in").click();
    await view.updateComplete;
    expect(button("Fit page width").getAttribute("aria-pressed")).toBe("false");
    const scroller = viewport();
    expect(scroller.scrollWidth).toBeGreaterThan(scroller.clientWidth);
    expect(scroller.scrollHeight).toBeGreaterThan(scroller.clientHeight);
    expect(scroller.getBoundingClientRect().bottom).toBeLessThanOrEqual(
      window.innerHeight,
    );
    expect(scroller.scrollTop).toBe(0);
    scroller.scrollLeft = 50;
    expect(scroller.scrollLeft).toBeGreaterThan(0);
    const menu = view.shadowRoot!.querySelector<HTMLElement>(".menu")!;
    const top = menu.getBoundingClientRect().top;
    scroller.scrollTop = 100;
    expect(scroller.scrollTop).toBeGreaterThan(0);
    expect(menu.getBoundingClientRect().top).toBe(top);
  });

  it("zooms out and back to the fitted width", async () => {
    await openDocument();
    await expect
      .poll(() => Math.abs(pageWidth() - (viewport().clientWidth - 1)))
      .toBeLessThan(1);
    const fitted = pageWidth();

    button("Zoom out").click();
    await view.updateComplete;
    expect(pageWidth()).toBeLessThan(fitted);

    button("Zoom in").click();
    await view.updateComplete;
    expect(pageWidth()).toBeCloseTo(fitted, 0);
  });

  it("stops zooming out at 30%", async () => {
    await openDocument();
    for (let i = 0; i < 20; i++) button("Zoom out").click();
    await view.updateComplete;
    expect(scale()).toBeCloseTo(0.3);
  });

  it("does not enlarge a page fitted below the minimum zoom", async () => {
    view.style.width = "200px";
    await openDocument();
    await expect
      .poll(() => Math.abs(pageWidth() - (viewport().clientWidth - 1)))
      .toBeLessThan(1);
    expect(scale()).toBeLessThan(0.3);
    const fitted = pageWidth();

    button("Zoom out").click();
    await view.updateComplete;
    expect(pageWidth()).toBeCloseTo(fitted, 0);

    button("Zoom in").click();
    await view.updateComplete;
    expect(pageWidth()).toBeGreaterThan(fitted);
  });

  it("starts the document and its errors below the menu", async () => {
    const menu = view.shadowRoot!.querySelector<HTMLElement>(".menu")!;
    const below = (selector: string) =>
      view.shadowRoot!.querySelector(selector)!.getBoundingClientRect().top -
      menu.getBoundingClientRect().bottom;
    await openDocument();
    for (let i = 0; i < 20; i++) button("Zoom out").click();
    await view.updateComplete;
    expect(below(".udf-document")).toBeGreaterThanOrEqual(0);
    await openDocument("<template>");
    expect(below('[role="alert"]')).toBeGreaterThanOrEqual(0);
  });

  it("replaces a previous document with an error for invalid input", async () => {
    await openDocument();
    await openDocument("<template>");
    expect(view.shadowRoot!.querySelector(".udf-document")).toBeNull();
    expect(
      view.shadowRoot!.querySelector('[role="alert"]')?.textContent,
    ).toBeTruthy();
    expect(button("Fit page width").disabled).toBe(true);
  });
});

describe("signature integrity", () => {
  const signed: SignatureReport = {
    status: "verified",
    signers: [
      {
        subject: "CN=Örnek İmzalayan",
        issuer: "CN=Örnek Sertifika",
        status: "verified",
      },
    ],
  };

  function panel() {
    return view.shadowRoot!.querySelector<HTMLElement>(".signature-panel");
  }

  it("shows integrity separately from certificate trust after the scaled document", async () => {
    expect(panel()).toBeNull();
    await openDocument(formattedXml, signed);
    expect(panel()?.querySelector('[role="status"]')?.textContent).toContain(
      "İmza bütünlüğü doğrulandı",
    );
    expect(panel()?.querySelector(".trust-note")?.textContent).toContain(
      "Sertifika güveni kontrol edilmedi",
    );
    const details = panel()!.querySelector("details")!;
    expect(details.open).toBe(false);
    details.querySelector("summary")!.click();
    expect(details.open).toBe(true);
    expect(details.textContent).toContain("Örnek İmzalayan");
    expect(details.textContent).toContain("iptal durumu");
    expect(details.textContent).toContain(
      "PDF çıktısı özgün UDF elektronik imzasını taşımaz",
    );
    expect(panel()?.closest(".content")).toBeNull();
    const content = view.shadowRoot!.querySelector(".content")!;
    expect(
      content.compareDocumentPosition(panel()!) &
        Node.DOCUMENT_POSITION_FOLLOWING,
    ).toBeTruthy();
  });

  it("replaces the previous signature result when opening unsigned or invalid documents", async () => {
    await openDocument(formattedXml, signed);
    await openDocument(formattedXml, { status: "unsigned", signers: [] });
    expect(panel()?.textContent).toContain("Elektronik imza yok");
    expect(panel()?.textContent).not.toContain("Örnek İmzalayan");
    expect(panel()?.querySelector(".trust-note")).toBeNull();
    await openDocument(formattedXml, { status: "invalid", signers: [] });
    expect(panel()?.querySelector('[role="alert"]')?.textContent).toContain(
      "İmza doğrulanamadı",
    );
    expect(view.shadowRoot!.querySelector(".udf-document")).not.toBeNull();
    await openDocument("<template>", signed);
    expect(panel()).toBeNull();
  });

  it("does not treat missing or unsupported results as unsigned or verified", async () => {
    for (const report of [
      undefined,
      { status: "unsupported", signers: [] } as SignatureReport,
    ]) {
      await openDocument(formattedXml, report);
      expect(panel()?.textContent).toContain("İmza denetlenemedi");
      expect(panel()?.textContent).not.toContain("İmza bütünlüğü doğrulandı");
    }
  });

  it("renders certificate fields as text, including mixed signer results", async () => {
    await openDocument(formattedXml, {
      status: "invalid",
      signers: [
        signed.signers[0],
        {
          subject: '<img src="x" onerror="alert(1)">',
          issuer: "<script>bad()</script>",
          status: "invalid",
        },
      ],
    });
    expect(panel()?.querySelectorAll("li").length).toBe(2);
    expect(panel()?.querySelector("img, script")).toBeNull();
    expect(panel()?.textContent).toContain('<img src="x"');
    expect(panel()?.dataset.status).toBe("invalid");
  });

  it("clears the previous document and signature when native opening fails", async () => {
    await openDocument(formattedXml, signed);
    const subscription = vi
      .mocked(listen)
      .mock.calls.find(([name]) => name === "open-error")!;
    subscription[1]({
      event: "open-error",
      id: 2,
      payload: "UDF arşivi okunamadı.",
    });
    await view.updateComplete;
    expect(panel()).toBeNull();
    expect(view.shadowRoot!.querySelector(".udf-document")).toBeNull();
    expect(button("Export PDF").disabled).toBe(true);
    expect(
      view.shadowRoot!.querySelector('[role="alert"]')?.textContent,
    ).toContain("UDF arşivi okunamadı");
  });
});

describe("pdf export", () => {
  it("needs a document before it can export one", async () => {
    expect(button("Export PDF").disabled).toBe(true);
    await openDocument();
    expect(button("Export PDF").disabled).toBe(false);
    await openDocument("<template>");
    expect(button("Export PDF").disabled).toBe(true);
  });

  it("sends the source document and releases the button", async () => {
    await openDocument();
    mockExport(() => []);
    button("Export PDF").click();
    await vi.waitFor(() =>
      expect(invoke).toHaveBeenCalledWith("export", { content: formattedXml }),
    );
    await flush();
    expect(busy()).toBe("false");
  });

  it("logs the renderer's limitations", async () => {
    await openDocument();
    mockExport(() => ["A font was substituted."]);
    button("Export PDF").click();
    await vi.waitFor(() =>
      expect(warn).toHaveBeenCalledWith(
        expect.stringContaining("A font was substituted."),
      ),
    );
  });

  it("ignores a second export while one is still running", async () => {
    await openDocument();
    let finish = (_: string[]) => {};
    mockExport(() => new Promise<string[]>((resolve) => (finish = resolve)));
    button("Export PDF").click();
    await vi.waitFor(() => expect(busy()).toBe("true"));
    button("Export PDF").click();
    await flush();
    expect(exportCalls()).toBe(1);
    finish([]);
    await vi.waitFor(() => expect(busy()).toBe("false"));
  });

  it("does nothing when there is no document to export", async () => {
    button("Export PDF").click();
    await flush();
    expect(exportCalls()).toBe(0);
  });

  it("logs a failed export and releases the button", async () => {
    await openDocument();
    mockExport(() => {
      throw new Error("Unable to open the PDF");
    });
    button("Export PDF").click();
    await vi.waitFor(() =>
      expect(error).toHaveBeenCalledWith(
        expect.stringContaining("Unable to open the PDF"),
      ),
    );
    await flush();
    expect(busy()).toBe("false");
  });
});

describe("session state", () => {
  async function restored() {
    const view = document.createElement("main-view");
    document.body.append(view);
    await view.updateComplete;
    return view;
  }

  function zoomOf(view: MainView) {
    return view.shadowRoot!.querySelector<HTMLElement>(".content")!.style.zoom;
  }

  it("restores a saved theme and zoom", async () => {
    localStorage.setItem("theme", "dark");
    localStorage.setItem("zoom", "5");
    const view = await restored();
    expect(document.documentElement.dataset.theme).toBe("dark");
    expect(zoomOf(view)).toBe("1.5");
    view.remove();
  });

  it("clamps an out-of-range zoom and ignores an unreadable one", async () => {
    localStorage.setItem("zoom", "99");
    const clamped = await restored();
    expect(zoomOf(clamped)).toBe("3");
    clamped.remove();

    localStorage.setItem("zoom", "-9");
    const raised = await restored();
    expect(parseFloat(zoomOf(raised))).toBeCloseTo(0.3);
    raised.remove();

    localStorage.setItem("zoom", "not a number");
    const ignored = await restored();
    expect(zoomOf(ignored)).toBe("1");
    ignored.remove();
  });

  it("switches and persists the theme", async () => {
    view
      .shadowRoot!.querySelector<HTMLInputElement>('input[value="dark"]')!
      .click();
    await view.updateComplete;
    expect(document.documentElement.dataset.theme).toBe("dark");
    expect(localStorage.getItem("theme")).toBe("dark");
    expect(
      view.shadowRoot!.querySelector<HTMLInputElement>('input[value="light"]')!
        .checked,
    ).toBe(false);
  });

  it("asks the backend for a file to open", () => {
    button("Open document").click();
    expect(invoke).toHaveBeenCalledWith("pick");
  });

  it("logs a failed startup handshake", async () => {
    vi.mocked(listen).mockRejectedValueOnce(new Error("no event bridge"));
    const failed = document.createElement("main-view");
    document.body.append(failed);
    await failed.updateComplete;
    await vi.waitFor(() =>
      expect(error).toHaveBeenCalledWith(
        expect.stringContaining("no event bridge"),
      ),
    );
    failed.remove();
  });

  it("keeps fitting the page after being re-attached", async () => {
    await openDocument();
    view.remove();
    document.body.append(view);
    await view.updateComplete;
    view.style.width = "500px";
    await expect
      .poll(() => Math.abs(pageWidth() - (viewport().clientWidth - 1)))
      .toBeLessThan(1);
  });
});
