import { describe, expect, it } from "vitest";
import formattedXml from "./fixtures/formatted.xml?raw";
import { renderUdf } from "../src/udf/render";

function udf(text: string, elements: string, styles = "", properties = "") {
  return `<template><content><![CDATA[${text}]]></content><properties>${properties}</properties><elements>${elements}</elements><styles>${styles}</styles></template>`;
}

describe("UDF rendering", () => {
  it("renders the demonstration document without losing text or formatting", () => {
    const { element, warnings } = renderUdf(formattedXml);
    expect(warnings).toEqual([]);
    expect(element.textContent).toBe(
      "Örnek BelgeBu metin kalın ve renklidir.AdıSoyadıAyşeYılmazSon paragraf.",
    );
    expect(element.querySelectorAll("tr")).toHaveLength(2);
    expect(element.querySelector("p")!.style.textAlign).toBe("center");
  });

  it("does not inherit structural dimensions or spans from ancestors", () => {
    const { element } = renderUdf(
      udf(
        "X",
        '<table width="500"><row height="60"><cell colspan="2"><table><row><cell><paragraph><content startOffset="0" length="1"/></paragraph></cell></row></table></cell></row></table>',
      ),
    );
    const tables = element.querySelectorAll("table");
    expect(tables[0].style.width).toBe("500pt");
    expect(tables[1].style.width).toBe("100%");
    expect(element.querySelectorAll("td")[1].colSpan).toBe(1);
    expect(element.querySelectorAll("tr")[1].style.height).toBe("");
  });

  it("preserves mixed paragraph/table order and Unicode text ranges", () => {
    const { element } = renderUdf(
      udf(
        "İ😀TabSon",
        `<paragraph><content startOffset="0" length="2"/></paragraph><table><row><cell><paragraph><content startOffset="2" length="3"/></paragraph></cell></row></table><paragraph><content startOffset="5" length="3"/></paragraph>`,
      ),
    );
    expect(Array.from(element.children, (c) => c.tagName)).toEqual([
      "P",
      "TABLE",
      "P",
    ]);
    expect(Array.from(element.children, (c) => c.textContent)).toEqual([
      "İ😀",
      "Tab",
      "Son",
    ]);
  });

  it("resolves named styles, inheritance and explicit false values", () => {
    const { element } = renderUdf(
      udf(
        "AB",
        `<paragraph resolver="heading" Alignment="1" SpaceAbove="6" FirstLineIndent="-12" LineSpacing="0.15"><content startOffset="0" length="1"/><content startOffset="1" length="1" bold="false" underline="false"/></paragraph>`,
        `<style name="base" family="Arial" size="16" bold="true" underline="true" foreground="-65536"/><style name="heading" resolver="base" italic="true"/>`,
      ),
    );
    const paragraph = element.querySelector("p")!;
    const [a, b] = element.querySelectorAll("span");
    expect(paragraph.style.textAlign).toBe("center");
    expect(paragraph.style.marginTop).toBe("6pt");
    expect(paragraph.style.textIndent).toBe("-12pt");
    expect(paragraph.style.lineHeight).toBe("1.15");
    expect(a.style.fontSize).toBe("16pt");
    expect(a.style.fontWeight).toBe("bold");
    expect(a.style.fontStyle).toBe("italic");
    expect(a.style.color).toBe("rgb(255, 0, 0)");
    expect(a.style.textDecoration).toBe("underline");
    expect(b.style.fontWeight).toBe("normal");
    expect(b.style.textDecoration).toBe("none");
    expect(paragraph.style.textDecoration).toBe("");
  });

  it("preserves tabs, spaces, empty paragraphs and paragraph delimiters", () => {
    const { element } = renderUdf(
      udf(
        "A \tB\n\n",
        `<paragraph><content startOffset="0" length="1"/><space startOffset="1" length="1"/><tab startOffset="2" length="1"/><content startOffset="3" length="2"/></paragraph><paragraph><content startOffset="5" length="1"/></paragraph>`,
      ),
    );
    expect(element.children[0].textContent).toBe("A \tB");
    expect(element.children[1].querySelector("br")).not.toBeNull();
  });

  it("renders table widths, merged cells, selective borders and nested tables", () => {
    const { element } = renderUdf(
      udf(
        "X",
        `<table columnSpans="100,200"><row><cell colspan="2" borderSpec="5" fillColor="16711680"><table><row><cell><paragraph><content startOffset="0" length="1"/></paragraph></cell></row></table></cell></row></table>`,
      ),
    );
    const cell = element.querySelector("td")!;
    expect(cell.colSpan).toBe(2);
    expect(cell.style.backgroundColor).toBe("rgb(255, 0, 0)");
    expect(cell.style.borderTopStyle).toBe("solid");
    expect(cell.style.borderRightStyle).not.toBe("solid");
    expect(element.querySelectorAll("table")).toHaveLength(2);
    expect(element.querySelector("col")!.style.width).toBe("100pt");
    expect(cell.textContent).toBe("X");
  });

  it("preserves borderless tables and cell border overrides", () => {
    const { element } = renderUdf(
      udf(
        "",
        '<table border="borderNone"><row><cell/><cell border="borderCell"/></row></table>',
      ),
    );
    const [borderless, bordered] = element.querySelectorAll("td");
    expect(borderless.style.borderTopStyle).not.toBe("solid");
    expect(bordered.style.borderTopStyle).toBe("solid");
  });

  it("uses page dimensions, margins, headers, footers and explicit page breaks", () => {
    const result = renderUdf(
      udf(
        "HBF",
        `<header><paragraph><content startOffset="0" length="1"/></paragraph></header><paragraph><content startOffset="1" length="1"/></paragraph><page-break/><footer><paragraph><content startOffset="2" length="1"/></paragraph></footer>`,
        "",
        `<pageFormat mediaSizeName="1" paperOrientation="0" topMargin="20" rightMargin="30" bottomMargin="40" leftMargin="50"/>`,
      ),
    );
    expect(result.element.style.width).toBe("297mm");
    expect(result.element.style.minHeight).toBe("210mm");
    expect(result.element.style.padding).toBe("20pt 30pt 40pt 50pt");
    expect(result.printStyle).toContain("size: 297mm 210mm");
    expect(result.element.querySelector("header")!.textContent).toBe("H");
    expect(result.element.querySelector("footer")!.textContent).toBe("F");
    expect(result.element.querySelector(".udf-page-break")).not.toBeNull();
  });

  it("renders only embedded raster images", () => {
    const { element, warnings } = renderUdf(
      udf(
        "",
        `<paragraph><image imageData="iVBORw0KGgo=" width="24" height="12"/><image imageData="https://example.com/image.png"/></paragraph>`,
      ),
    );
    const [embedded, external] = element.querySelectorAll("img");
    expect(embedded.src).toBe("data:image/png;base64,iVBORw0KGgo=");
    expect(embedded.style.width).toBe("24pt");
    expect(external.hasAttribute("src")).toBe(false);
    expect(warnings).toContain("An unsupported image could not be displayed.");
  });

  it("never interprets document text, attributes or unknown XML as HTML/CSS", () => {
    const text = '<img src=x onerror="alert(1)">';
    const { element } = renderUdf(
      udf(
        text,
        `<script>alert(1)</script><paragraph family="Arial&quot;; background-image: url(https://example.com); color:red" onclick="alert(1)" style="display:none"><content startOffset="0" length="${text.length}"/></paragraph>`,
      ),
    );
    expect(element.textContent).toBe(text);
    expect(element.querySelector("script, img, [onclick]")).toBeNull();
    const paragraph = element.querySelector("p")!;
    expect(paragraph.style.backgroundImage).toBe("");
    expect(paragraph.style.display).toBe("");
    expect(paragraph.style.color).toBe("rgb(0, 0, 0)");
  });

  it("rejects malformed XML, missing content and DTDs", () => {
    for (const xml of [
      "<template>",
      "<html/>",
      "<template/>",
      '<!DOCTYPE template [<!ENTITY x "test">]><template><content>&x;</content></template>',
    ]) {
      expect(() => renderUdf(xml)).toThrow();
    }
  });

  it("falls back to escaped plain text when layout is absent", () => {
    const result = renderUdf(udf("Hello <world>\n", ""));
    expect(result.element.textContent).toBe("Hello <world>\n");
    expect(result.element.querySelector("world")).toBeNull();
    expect(result.warnings).toHaveLength(1);
  });

  it("reports invalid ranges and cyclic styles without hanging", () => {
    const result = renderUdf(
      udf(
        "A",
        '<paragraph resolver="loop"><content startOffset="-1" length="2"/></paragraph>',
        '<style name="loop" resolver="loop"/>',
      ),
    );
    expect(result.element.textContent).toBe("");
    expect(result.warnings).toHaveLength(2);
  });
  it("raises superscript and lowers subscript text", () => {
    const { element } = renderUdf(
      udf(
        "ab",
        '<paragraph><content startOffset="0" length="1" superscript="true"/><content startOffset="1" length="1" subscript="true"/></paragraph>',
      ),
    );
    const [raised, lowered] = element.querySelectorAll("span");
    expect(raised.style.verticalAlign).toBe("super");
    expect(lowered.style.verticalAlign).toBe("sub");
  });

  it("renders tabs, spaces and fields that carry no text offset", () => {
    const { element } = renderUdf(
      udf(
        "A",
        '<paragraph><content startOffset="0" length="1"/><tab/><space/><field/></paragraph>',
      ),
    );
    expect(element.textContent).toBe("A\t ");
  });

  it("recognises JPEG data and rejects malformed image data", () => {
    const { element, warnings } = renderUdf(
      udf(
        "",
        '<paragraph><image imageData="/9j/4AAQ"/><image imageData="iVBORw0KGgo!!"/></paragraph>',
      ),
    );
    const [jpeg, malformed] = element.querySelectorAll("img");
    expect(jpeg.src).toBe("data:image/jpeg;base64,/9j/4AAQ");
    expect(malformed.hasAttribute("src")).toBe(false);
    expect(warnings).toContain("An unsupported image could not be displayed.");
  });

  it("warns about custom tab stops and automatic list markers", () => {
    const { warnings } = renderUdf(
      udf(
        "A",
        '<paragraph TabSet="120" Bulleted="true" SpaceBelow="8" LeftIndent="10" RightIndent="12"><content startOffset="0" length="1"/></paragraph>',
      ),
    );
    expect(warnings).toContain(
      "Custom tab stops are approximated using standard tabs.",
    );
    expect(warnings).toContain("Automatic list markers are not yet supported.");
  });

  it("outlines a bordered table and aligns cell content", () => {
    const { element } = renderUdf(
      udf(
        "AB",
        '<table border="borderTable"><row><cell align="vcenter" width="80"><paragraph><content startOffset="0" length="1"/></paragraph></cell><cell align="bottom"><paragraph><content startOffset="1" length="1"/></paragraph></cell></row></table>',
      ),
    );
    expect(element.querySelector("table")!.style.border).toBe(
      "0.75pt solid black",
    );
    const [middle, bottom] = element.querySelectorAll("td");
    expect(middle.style.verticalAlign).toBe("middle");
    expect(middle.style.width).toBe("80pt");
    expect(bottom.style.verticalAlign).toBe("bottom");
  });

  it("applies named cell border styles and falls back to solid", () => {
    const { element } = renderUdf(
      udf(
        "AB",
        '<table><row><cell borderStyle="borderStyle-dashed"><paragraph><content startOffset="0" length="1"/></paragraph></cell><cell borderStyle="squiggly"><paragraph><content startOffset="1" length="1"/></paragraph></cell></row></table>',
      ),
    );
    const [dashed, fallback] = element.querySelectorAll("td");
    expect(dashed.style.borderTopStyle).toBe("dashed");
    expect(fallback.style.borderTopStyle).toBe("solid");
  });

  it("maps the known paper sizes and falls back to A4", () => {
    const width = (mediaSizeName: string) =>
      renderUdf(
        udf(
          "A",
          '<paragraph><content startOffset="0" length="1"/></paragraph>',
          "",
          `<pageFormat mediaSizeName="${mediaSizeName}"/>`,
        ),
      ).element.style.width;
    expect(width("5")).toBe("215.9mm");
    expect(width("6")).toBe("215.9mm");
    expect(width("11")).toBe("148mm");
    expect(width("99")).toBe("210mm");
  });

  it("refuses documents nested too deeply to display", () => {
    const deep = "<paragraph>".repeat(120) + "</paragraph>".repeat(120);
    expect(() => renderUdf(udf("A", deep))).toThrow(/nested too deeply/);
  });
});
