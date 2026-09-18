/*
 * Copyright (c) 1986-2026 Ecmel Ercan
 * See LICENSE in the project root for license information.
 */

type Attributes = Record<string, string>;

export interface RenderedDocument {
  element: HTMLElement;
  printStyle: string;
  warnings: string[];
}

function attributes(element: Element): Attributes {
  return Object.fromEntries(
    Array.from(element.attributes, (a) => [a.name, a.value]),
  );
}

function number(
  value: string | undefined,
  fallback: number,
  min = 0,
  max = 10000,
): number {
  const parsed = value?.trim() ? Number(value) : NaN;
  return Number.isFinite(parsed) && parsed >= min && parsed <= max
    ? parsed
    : fallback;
}

function color(value: string | undefined): string {
  if (!value || !/^-?\d+$/.test(value)) return "";
  return `#${(Number(value) & 0xffffff).toString(16).padStart(6, "0")}`;
}

function applyStyle(element: HTMLElement, attrs: Attributes) {
  const s = element.style;
  // Property assignment prevents an attribute from injecting extra CSS declarations.
  if (attrs.family)
    s.fontFamily = `"${attrs.family.replace(/["\\\n\r]/g, "")}", serif`;
  if (attrs.size) s.fontSize = `${number(attrs.size, 12, 1, 500)}pt`;
  if (attrs.bold) s.fontWeight = attrs.bold === "true" ? "bold" : "normal";
  if (attrs.italic) s.fontStyle = attrs.italic === "true" ? "italic" : "normal";
  if (element.tagName === "SPAN")
    s.textDecoration =
      [
        attrs.underline === "true" ? "underline" : "",
        attrs.strikeThrough === "true" ? "line-through" : "",
      ]
        .filter(Boolean)
        .join(" ") || "none";
  s.color = color(attrs.foreground);
  s.backgroundColor = color(attrs.background);
  if (attrs.superscript === "true") s.verticalAlign = "super";
  if (attrs.subscript === "true") s.verticalAlign = "sub";
}

/** Render content.xml without executing document markup, CSS, or remote resources. */
export function renderUdf(xml: string): RenderedDocument {
  const parsed = new DOMParser().parseFromString(xml, "application/xml");
  if (
    parsed.querySelector("parsererror") ||
    parsed.doctype ||
    parsed.documentElement.tagName !== "template"
  ) {
    throw new Error("This document contains invalid or unsupported UDF XML.");
  }
  const root = parsed.documentElement;
  const child = (parent: Element, name: string) =>
    Array.from(parent.children).find((e) => e.tagName === name);
  const content = child(root, "content");
  if (!content) throw new Error("This UDF document has no text content.");
  const rawText = content.textContent || "";
  // UDF's public parsers use Unicode codepoint offsets, including supplementary characters.
  const characters = Array.from(rawText);
  const warnings = new Set<string>();
  const styles = new Map<string, Attributes>();
  for (const style of Array.from(child(root, "styles")?.children || [])) {
    if (style.tagName === "style" && style.hasAttribute("name"))
      styles.set(style.getAttribute("name")!, attributes(style));
  }
  function namedStyle(name: string, seen = new Set<string>()): Attributes {
    if (seen.has(name)) {
      warnings.add("Cyclic style references were ignored.");
      return {};
    }
    seen.add(name);
    const style = styles.get(name) || {};
    return {
      ...(style.resolver ? namedStyle(style.resolver, seen) : {}),
      ...style,
    };
  }
  function resolved(element: Element, inherited: Attributes): Attributes {
    const attrs = attributes(element);
    const styleKeys = [
      "family",
      "size",
      "bold",
      "italic",
      "underline",
      "strikeThrough",
      "foreground",
      "background",
      "superscript",
      "subscript",
      "Alignment",
      "SpaceAbove",
      "SpaceBelow",
      "LeftIndent",
      "RightIndent",
      "FirstLineIndent",
      "LineSpacing",
      "TabSet",
    ];
    const parentStyle = Object.fromEntries(
      Object.entries(inherited).filter(([key]) => styleKeys.includes(key)),
    );
    return {
      ...parentStyle,
      ...(attrs.resolver ? namedStyle(attrs.resolver) : {}),
      ...attrs,
    };
  }
  function textFor(node: Element): string {
    const start = number(
      node.getAttribute("startOffset") || undefined,
      -1,
      0,
      characters.length,
    );
    const length = number(
      node.getAttribute("length") || undefined,
      -1,
      0,
      characters.length,
    );
    if (
      !Number.isInteger(start) ||
      !Number.isInteger(length) ||
      start < 0 ||
      length < 0 ||
      start + length > characters.length
    ) {
      warnings.add(
        "Some text ranges are invalid; the document may be incomplete.",
      );
      return "";
    }
    return characters.slice(start, start + length).join("");
  }
  function render(node: Element, inherited: Attributes, depth = 0): Node {
    if (depth > 100)
      throw new Error("This document is nested too deeply to display.");
    const attrs = resolved(node, inherited);
    const tag = node.tagName;
    const children = () =>
      Array.from(node.children, (c) => render(c, attrs, depth + 1));
    if (["content", "field", "space", "tab"].includes(tag)) {
      const span = document.createElement("span");
      applyStyle(span, attrs);
      span.textContent = node.hasAttribute("startOffset")
        ? textFor(node)
        : tag === "tab"
          ? "\t"
          : tag === "space"
            ? " "
            : "";
      return span;
    }
    if (tag === "image") {
      const image = document.createElement("img");
      const data = (attrs.imageData || "").replace(/\s/g, "");
      const mime = data.startsWith("iVBORw0KGgo")
        ? "image/png"
        : data.startsWith("/9j/")
          ? "image/jpeg"
          : "";
      if (mime && /^[A-Za-z0-9+/]*={0,2}$/.test(data)) {
        image.src = `data:${mime};base64,${data}`;
        image.alt = "Document image";
        if (attrs.width) image.style.width = `${number(attrs.width, 100)}pt`;
        if (attrs.height) image.style.height = `${number(attrs.height, 100)}pt`;
      } else {
        image.alt = "[Unsupported document image]";
        warnings.add("An unsupported image could not be displayed.");
      }
      return image;
    }
    const tags: Record<string, string> = {
      paragraph: "p",
      table: "table",
      row: "tr",
      cell: "td",
      header: "header",
      footer: "footer",
      "page-break": "div",
    };
    if (!tags[tag]) {
      warnings.add(`Unsupported document element: ${tag}.`);
      const fragment = document.createDocumentFragment();
      fragment.append(...children());
      return fragment;
    }
    const element = document.createElement(tags[tag]);
    applyStyle(element, attrs);
    if (tag === "paragraph") {
      element.style.textAlign =
        ["left", "center", "right", "justify"][
          number(attrs.Alignment, 0, 0, 3)
        ] || "left";
      element.style.marginTop = `${number(attrs.SpaceAbove, 0)}pt`;
      element.style.marginBottom = `${number(attrs.SpaceBelow, 0)}pt`;
      element.style.paddingLeft = `${number(attrs.LeftIndent, 0)}pt`;
      element.style.paddingRight = `${number(attrs.RightIndent, 0)}pt`;
      element.style.textIndent = `${number(attrs.FirstLineIndent, 0, -1000)}pt`;
      element.style.lineHeight = String(
        1 + number(attrs.LineSpacing, 0, 0, 10),
      );
      if (attrs.TabSet)
        warnings.add("Custom tab stops are approximated using standard tabs.");
      if (attrs.Numbered === "true" || attrs.Bulleted === "true")
        warnings.add("Automatic list markers are not yet supported.");
    }
    if (tag === "table") {
      if (attrs.border === "borderTable")
        element.style.border = "0.75pt solid black";
      element.style.width = attrs.width
        ? `${number(attrs.width, 400, 1)}pt`
        : "100%";
      if (attrs.columnSpans) {
        const columns = document.createElement("colgroup");
        for (const width of attrs.columnSpans.split(",")) {
          const column = document.createElement("col");
          column.style.width = `${number(width, 100, 1)}pt`;
          columns.append(column);
        }
        element.append(columns);
      }
    }
    if (tag === "row" && attrs.height)
      element.style.height = `${number(attrs.height, 0)}pt`;
    if (tag === "cell") {
      const cell = element as HTMLTableCellElement;
      cell.colSpan = number(attrs.colspan || attrs.colSpan, 1, 1, 1000);
      cell.rowSpan = number(attrs.rowspan || attrs.rowSpan, 1, 1, 1000);
      cell.style.verticalAlign =
        attrs.align === "vcenter"
          ? "middle"
          : attrs.align === "bottom"
            ? "bottom"
            : "top";
      if (attrs.width) cell.style.width = `${number(attrs.width, 100, 1)}pt`;
      cell.style.backgroundColor = color(attrs.fillColor || attrs.background);
      const borderMode =
        attrs.border ||
        node.closest("row")?.getAttribute("border") ||
        node.closest("table")?.getAttribute("border");
      const borders =
        borderMode === "borderNone" || borderMode === "borderTable"
          ? 0
          : number(attrs.borderSpec || attrs.borders, 15, 0, 15);
      const borderStyle = (attrs.borderStyle || "").replace("borderStyle-", "");
      const line = ["solid", "dotted", "dashed", "double"].includes(borderStyle)
        ? borderStyle
        : "solid";
      ["top", "right", "bottom", "left"].forEach((side, index) => {
        cell.style.setProperty(
          `border-${side}`,
          borders & (1 << index)
            ? `${number(attrs.borderWidth, 0.75)}pt ${line} ${color(attrs.borderColor) || "#000000"}`
            : "none",
        );
      });
    }
    if (tag === "page-break") element.className = "udf-page-break";
    element.append(...children());
    if (tag === "paragraph") {
      // A trailing newline is the paragraph delimiter, not an additional blank line.
      const last = element.lastChild;
      if (last instanceof HTMLSpanElement && last.textContent?.endsWith("\n"))
        last.textContent = last.textContent.slice(0, -1);
      if (!element.textContent && !element.querySelector("img"))
        element.append(document.createElement("br"));
    }
    return element;
  }

  const page = attributes(
    child(child(root, "properties") || root, "pageFormat") ||
      document.createElement("div"),
  );
  const paperSizes: Record<string, [number, number]> = {
    "1": [210, 297],
    "5": [215.9, 279.4],
    "6": [215.9, 355.6],
    "11": [148, 210],
  };
  let [width, height] = paperSizes[page.mediaSizeName] || paperSizes["1"];
  if (page.paperOrientation === "0") [width, height] = [height, width];
  const margins = ["top", "right", "bottom", "left"]
    .map((side) => `${number(page[`${side}Margin`], 42.52, 0, 300)}pt`)
    .join(" ");
  const element = document.createElement("article");
  element.className = "udf-document";
  element.lang = "tr";
  element.style.width = `${width}mm`;
  element.style.minHeight = `${height}mm`;
  element.style.padding = margins;
  const elements = child(root, "elements");
  const defaults = {
    family: "Times New Roman",
    size: "12",
    foreground: "-16777216",
    ...namedStyle("default"),
  };
  if (elements?.children.length) {
    const inherited = resolved(elements, defaults);
    element.append(
      ...Array.from(elements.children, (node) => render(node, inherited)),
    );
  } else {
    const fallback = document.createElement("div");
    fallback.textContent = rawText;
    element.append(fallback);
    warnings.add("No document layout was found; showing plain text.");
  }
  return {
    element,
    printStyle: `@page { size: ${width}mm ${height}mm; margin: ${margins}; }`,
    warnings: [...warnings],
  };
}
