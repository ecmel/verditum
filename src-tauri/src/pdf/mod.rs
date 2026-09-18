/*
 * Copyright (c) 1986-2026 Ecmel Ercan
 * See LICENSE in the project root for license information.
 */

mod fonts;
mod model;

use base64::Engine;
use fonts::Fonts;
use lopdf::content::{Content, Operation};
use lopdf::{dictionary, Dictionary, Document, Object, Stream, StringFormat};
use model::Element;
use std::collections::BTreeSet;

#[derive(Clone)]
enum Draw {
  Glyph {
    x: f32,
    y: f32,
    font: usize,
    cid: u16,
    size: f32,
    color: [f32; 3],
  },
  Rect {
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    color: [f32; 3],
    fill: bool,
  },
  Image {
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    name: String,
  },
}
impl Draw {
  fn offset(&mut self, dx: f32, dy: f32) {
    match self {
      Self::Glyph { x, y, .. }
      | Self::Rect { x, y, .. }
      | Self::Image { x, y, .. } => {
        *x += dx;
        *y += dy;
      }
    }
  }
}
struct Band {
  height: f32,
  draw: Vec<Draw>,
  page_break: bool,
}
impl Band {
  fn empty(height: f32) -> Self {
    Self {
      height,
      draw: vec![],
      page_break: false,
    }
  }
}
#[derive(Clone)]
struct Atom {
  width: f32,
  height: f32,
  space: bool,
  newline: bool,
  draw: Vec<Draw>,
}
struct Renderer {
  pdf: Document,
  fonts: Fonts,
  images: Dictionary,
  warnings: BTreeSet<String>,
  atoms: usize,
}
fn color(value: &str) -> [f32; 3] {
  let v = value.parse::<i64>().unwrap_or(0) as u32;
  [
    ((v >> 16) & 255) as f32 / 255.,
    ((v >> 8) & 255) as f32 / 255.,
    (v & 255) as f32 / 255.,
  ]
}
impl Renderer {
  fn inline(
    &mut self,
    e: &Element,
    out: &mut Vec<Atom>,
    width: f32,
  ) -> Result<(), String> {
    if e.tag == "image" {
      let encoded: String = e
        .get("imageData")
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect();
      let bytes = base64::engine::general_purpose::STANDARD
        .decode(encoded)
        .map_err(|_| "Invalid UDF image data.")?;
      let mut reader = image::ImageReader::new(std::io::Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|e| e.to_string())?;
      let mut limits = image::Limits::default();
      limits.max_image_width = Some(10000);
      limits.max_image_height = Some(10000);
      limits.max_alloc = Some(128 * 1024 * 1024);
      reader.limits(limits);
      let img = reader
        .decode()
        .map_err(|e| format!("Cannot decode UDF image: {e}"))?
        .to_rgba8();
      let (iw, ih) = img.dimensions();
      let mut rgb = Vec::with_capacity(iw as usize * ih as usize * 3);
      let mut alpha = Vec::with_capacity(iw as usize * ih as usize);
      for pixel in img.pixels() {
        rgb.extend_from_slice(&pixel.0[..3]);
        alpha.push(pixel.0[3]);
      }
      let mask = self.pdf.add_object(Stream::new(dictionary! { "Type" => "XObject", "Subtype" => "Image", "Width" => iw, "Height" => ih, "ColorSpace" => "DeviceGray", "BitsPerComponent" => 8 }, alpha));
      let id = self.pdf.add_object(Stream::new(dictionary! { "Type" => "XObject", "Subtype" => "Image", "Width" => iw, "Height" => ih, "ColorSpace" => "DeviceRGB", "BitsPerComponent" => 8, "SMask" => mask }, rgb));
      let name = format!("Im{}", self.images.len());
      self.images.set(name.clone(), id);
      let mut w = e.num("width", iw as f32 * 0.75).max(1.);
      let mut h = e
        .num("height", ih as f32 * 0.75 * w / (iw as f32 * 0.75))
        .max(1.);
      if w > width {
        h *= width / w;
        w = width;
      }
      out.push(Atom {
        width: w,
        height: h,
        space: false,
        newline: false,
        draw: vec![Draw::Image {
          x: 0.,
          y: 0.,
          w,
          h,
          name,
        }],
      });
    } else if matches!(e.tag.as_str(), "content" | "field" | "space" | "tab") {
      let original_size = e.num("size", 12.).clamp(1., 500.);
      let superscript = e.get("superscript") == "true";
      let subscript = e.get("subscript") == "true";
      let size =
        original_size * if superscript || subscript { 0.7 } else { 1. };
      for ch in e.text.chars().filter(|c| *c != '\r') {
        self.atoms += 1;
        if self.atoms > 500_000 {
          return Err("Document exceeds the PDF export text limit.".into());
        }
        if ch == '\n' {
          out.push(Atom {
            width: 0.,
            height: original_size * 1.2,
            space: false,
            newline: true,
            draw: vec![],
          });
          continue;
        }
        if ch.is_control() && ch != '\t' {
          return Err(format!(
            "Unsupported control character U+{:04X} in UDF.",
            ch as u32
          ));
        }
        let (font, cid, advance) = self.fonts.glyph(
          e.get("family"),
          e.get("bold") == "true",
          e.get("italic") == "true",
          if ch == '\t' { ' ' } else { ch },
        )?;
        let w = advance * size * if ch == '\t' { 4. } else { 1. };
        let baseline = original_size * 0.95
          + if superscript {
            -original_size * 0.3
          } else if subscript {
            original_size * 0.15
          } else {
            0.
          };
        let mut draw = vec![];
        if !e.get("background").is_empty() {
          draw.push(Draw::Rect {
            x: 0.,
            y: 0.,
            w,
            h: original_size * 1.2,
            color: color(e.get("background")),
            fill: true,
          });
        }
        if ch != '\t' {
          draw.push(Draw::Glyph {
            x: 0.,
            y: baseline,
            font,
            cid,
            size,
            color: color(e.get("foreground")),
          });
        }
        for (key, y) in [
          ("underline", baseline + size * 0.1),
          ("strikeThrough", baseline - size * 0.3),
        ] {
          if e.get(key) == "true" {
            draw.push(Draw::Rect {
              x: 0.,
              y,
              w,
              h: (size / 20.).max(0.4),
              color: color(e.get("foreground")),
              fill: true,
            });
          }
        }
        out.push(Atom {
          width: w,
          height: original_size * 1.2,
          space: ch == ' ' || ch == '\t',
          newline: false,
          draw,
        });
      }
    } else {
      if !matches!(e.tag.as_str(), "paragraph" | "cell") {
        self.warnings.insert(format!(
          "Unsupported inline element '{}' was flattened.",
          e.tag
        ));
      }
      for child in &e.children {
        self.inline(child, out, width)?;
      }
    }
    Ok(())
  }
  fn paragraph(
    &mut self,
    e: &Element,
    width: f32,
  ) -> Result<Vec<Band>, String> {
    for (key, message) in [
      ("TabSet", "Custom tab stops use four spaces in PDF output."),
      (
        "Numbered",
        "Automatic list numbering is not supported in PDF output.",
      ),
      (
        "Bulleted",
        "Automatic bullets are not supported in PDF output.",
      ),
    ] {
      if !e.get(key).is_empty() && e.get(key) != "false" {
        self.warnings.insert(message.into());
      }
    }
    let left = e.num("LeftIndent", 0.).max(0.);
    let right = e.num("RightIndent", 0.).max(0.);
    let first = e.num("FirstLineIndent", 0.);
    let mut atoms = vec![];
    self.inline(e, &mut atoms, (width - left - right).max(1.))?;
    if atoms.last().is_some_and(|a| a.newline) {
      atoms.pop();
    }
    let mut bands = vec![];
    let mut start = 0;
    let mut first_line = true;
    loop {
      let indent = if first_line { first } else { 0. };
      let available = width - left - right - indent;
      if available < 1. {
        return Err("Paragraph indents leave no room for text.".into());
      }
      let mut end = start;
      let mut used = 0.;
      let mut last_space = None;
      while end < atoms.len() && !atoms[end].newline {
        if used + atoms[end].width > available && end > start {
          break;
        }
        if atoms[end].width > available {
          return Err("A glyph or image is wider than the paragraph.".into());
        }
        used += atoms[end].width;
        if atoms[end].space {
          last_space = Some(end + 1);
        }
        end += 1;
      }
      if end < atoms.len() && !atoms[end].newline {
        if let Some(boundary) = last_space {
          end = boundary;
        }
      }
      let line = &atoms[start..end];
      let used: f32 = line.iter().map(|a| a.width).sum();
      let height = line
        .iter()
        .map(|a| a.height)
        .fold(e.num("size", 12.).clamp(1., 500.) * 1.2, f32::max)
        * (1. + e.num("LineSpacing", 0.).clamp(0., 10.));
      let final_line =
        end == atoms.len() || atoms.get(end).is_some_and(|a| a.newline);
      let spaces = line.iter().filter(|a| a.space).count();
      let extra = if e.get("Alignment") == "3" && !final_line && spaces > 0 {
        (available - used).max(0.) / spaces as f32
      } else {
        0.
      };
      let mut x = left
        + indent
        + match e.get("Alignment") {
          "1" => (available - used) / 2.,
          "2" => available - used,
          _ => 0.,
        };
      let mut band = Band::empty(height);
      for atom in line {
        for mut draw in atom.draw.clone() {
          draw.offset(x, 0.);
          band.draw.push(draw);
        }
        x += atom.width + if atom.space { extra } else { 0. };
      }
      if first_line {
        let above = e.num("SpaceAbove", 0.).max(0.);
        for d in &mut band.draw {
          d.offset(0., above);
        }
        band.height += above;
      }
      bands.push(band);
      if end == atoms.len() {
        break;
      }
      start = end + usize::from(atoms[end].newline);
      first_line = false;
    }
    if let Some(last) = bands.last_mut() {
      last.height += e.num("SpaceBelow", 0.).max(0.);
    }
    Ok(bands)
  }
  fn blocks(
    &mut self,
    elements: &[Element],
    width: f32,
    depth: usize,
  ) -> Result<Vec<Band>, String> {
    if depth > 100 {
      return Err("PDF layout nesting limit exceeded.".into());
    }
    let mut result = vec![];
    for e in elements {
      match e.tag.as_str() {
        "paragraph" => result.extend(self.paragraph(e, width)?),
        "table" => result.extend(self.table(e, width, depth + 1)?),
        "page-break" => {
          let mut band = Band::empty(0.);
          band.page_break = true;
          result.push(band);
          result.extend(self.blocks(&e.children, width, depth + 1)?);
        }
        "content" | "field" | "space" | "tab" | "image" => {
          let wrapper = Element {
            tag: "paragraph".into(),
            attrs: e.attrs.clone(),
            text: String::new(),
            children: vec![e.clone()],
          };
          result.extend(self.paragraph(&wrapper, width)?);
        }
        _ => {
          if !matches!(e.tag.as_str(), "header" | "footer" | "cell") {
            self
              .warnings
              .insert(format!("Unsupported block '{}' was flattened.", e.tag));
          }
          result.extend(self.blocks(&e.children, width, depth + 1)?);
        }
      }
    }
    Ok(result)
  }
  fn table(
    &mut self,
    table: &Element,
    available: f32,
    depth: usize,
  ) -> Result<Vec<Band>, String> {
    let width = table.num("width", available).clamp(1., available);
    let span = |cell: &Element, lower: &str, camel: &str| {
      cell.num(lower, cell.num(camel, 1.)).clamp(1., 1000.) as usize
    };
    let columns = table
      .children
      .iter()
      .map(|row| {
        row
          .children
          .iter()
          .map(|c| span(c, "colspan", "colSpan"))
          .sum::<usize>()
      })
      .max()
      .unwrap_or(0);
    if columns == 0 {
      return Ok(vec![]);
    }
    if columns > 1000 {
      return Err("Too many table columns.".into());
    }
    let specified: Vec<f32> = table
      .get("columnSpans")
      .split(',')
      .filter_map(|s| s.trim().parse::<f32>().ok())
      .filter(|n| n.is_finite() && *n > 0.)
      .collect();
    let widths = if specified.len() == columns {
      let sum: f32 = specified.iter().sum();
      specified
        .iter()
        .map(|v| v * width / sum)
        .collect::<Vec<_>>()
    } else {
      vec![width / columns as f32; columns]
    };
    let mut bands = vec![];
    for row in &table.children {
      if row.tag != "row" {
        return Err("Unsupported table structure.".into());
      }
      let mut cells = vec![];
      let mut column = 0;
      let mut height = row.num("height", 0.).max(0.);
      for cell in &row.children {
        if cell.tag != "cell" {
          return Err("Unsupported table row structure.".into());
        }
        if span(cell, "rowspan", "rowSpan") > 1 {
          return Err(
            "PDF export does not yet support cells spanning multiple rows."
              .into(),
          );
        }
        let count = span(cell, "colspan", "colSpan");
        let w: f32 = widths[column..column + count].iter().sum();
        column += count;
        if w <= 10.8 {
          return Err("Table cell is too narrow for PDF output.".into());
        }
        let content = self.blocks(&cell.children, w - 10.8, depth + 1)?;
        if content.iter().any(|b| b.page_break) {
          return Err(
            "Page breaks inside table cells are not yet supported.".into(),
          );
        }
        let h: f32 = content.iter().map(|b| b.height).sum();
        height = height.max(h + 4.);
        cells.push((cell, w, h, content));
      }
      let mut band = Band::empty(height);
      let mut x = 0.;
      for (cell, w, h, content) in cells {
        let fill = if !cell.get("fillColor").is_empty() {
          cell.get("fillColor")
        } else {
          cell.get("background")
        };
        if !fill.is_empty() {
          band.draw.push(Draw::Rect {
            x,
            y: 0.,
            w,
            h: height,
            color: color(fill),
            fill: true,
          });
        }
        let mut y = match cell.get("align") {
          "vcenter" => (height - h) / 2.,
          "bottom" => height - h - 2.,
          _ => 2.,
        };
        for b in content {
          for mut d in b.draw {
            d.offset(x + 5.4, y);
            band.draw.push(d);
          }
          y += b.height;
        }
        let mode = [cell.get("border"), row.get("border"), table.get("border")]
          .into_iter()
          .find(|s| !s.is_empty())
          .unwrap_or("");
        let borders = if mode == "borderNone" || mode == "borderTable" {
          0
        } else {
          cell
            .num("borderSpec", cell.num("borders", 15.))
            .clamp(0., 15.) as u8
        };
        let thickness = cell.num("borderWidth", 0.75).clamp(0., 20.);
        if !cell.get("borderStyle").is_empty()
          && cell.get("borderStyle") != "borderStyle-solid"
          && cell.get("borderStyle") != "solid"
        {
          self
            .warnings
            .insert("Table borders use solid lines in PDF output.".into());
        }
        for (bit, dx, dy, bw, bh) in [
          (1, 0., 0., w, thickness),
          (2, w - thickness, 0., thickness, height),
          (4, 0., height - thickness, w, thickness),
          (8, 0., 0., thickness, height),
        ] {
          if borders & bit != 0 {
            band.draw.push(Draw::Rect {
              x: x + dx,
              y: dy,
              w: bw,
              h: bh,
              color: color(cell.get("borderColor")),
              fill: true,
            });
          }
        }
        x += w;
      }
      if table.get("border") == "borderTable" {
        band.draw.push(Draw::Rect {
          x: 0.,
          y: 0.,
          w: 0.75,
          h: height,
          color: [0.; 3],
          fill: true,
        });
        band.draw.push(Draw::Rect {
          x: width - 0.75,
          y: 0.,
          w: 0.75,
          h: height,
          color: [0.; 3],
          fill: true,
        });
      }
      bands.push(band);
    }
    if table.get("border") == "borderTable" {
      if let Some(first) = bands.first_mut() {
        first.draw.push(Draw::Rect {
          x: 0.,
          y: 0.,
          w: width,
          h: 0.75,
          color: [0.; 3],
          fill: true,
        });
      }
      if let Some(last) = bands.last_mut() {
        last.draw.push(Draw::Rect {
          x: 0.,
          y: last.height - 0.75,
          w: width,
          h: 0.75,
          color: [0.; 3],
          fill: true,
        });
      }
    }
    Ok(bands)
  }
}
fn operations(draws: &[Draw], page_height: f32) -> Vec<Operation> {
  let mut ops = vec![];
  for d in draws {
    ops.push(Operation::new("q", vec![]));
    match d {
      Draw::Glyph {
        x,
        y,
        font,
        cid,
        size,
        color,
      } => {
        ops.push(Operation::new(
          "rg",
          color.iter().map(|v| Object::Real(*v)).collect(),
        ));
        ops.push(Operation::new("BT", vec![]));
        ops.push(Operation::new(
          "Tf",
          vec![
            Object::Name(format!("F{font}").into_bytes()),
            (*size).into(),
          ],
        ));
        ops.push(Operation::new(
          "Tm",
          vec![
            1.into(),
            0.into(),
            0.into(),
            1.into(),
            (*x).into(),
            (page_height - y).into(),
          ],
        ));
        ops.push(Operation::new(
          "Tj",
          vec![Object::String(
            cid.to_be_bytes().to_vec(),
            StringFormat::Hexadecimal,
          )],
        ));
        ops.push(Operation::new("ET", vec![]));
      }
      Draw::Rect {
        x,
        y,
        w,
        h,
        color,
        fill,
      } => {
        ops.push(Operation::new(
          if *fill { "rg" } else { "RG" },
          color.iter().map(|v| Object::Real(*v)).collect(),
        ));
        ops.push(Operation::new(
          "re",
          vec![
            (*x).into(),
            (page_height - y - h).into(),
            (*w).into(),
            (*h).into(),
          ],
        ));
        ops.push(Operation::new(if *fill { "f" } else { "S" }, vec![]));
      }
      Draw::Image { x, y, w, h, name } => {
        ops.push(Operation::new(
          "cm",
          vec![
            (*w).into(),
            0.into(),
            0.into(),
            (*h).into(),
            (*x).into(),
            (page_height - y - h).into(),
          ],
        ));
        ops.push(Operation::new(
          "Do",
          vec![Object::Name(name.as_bytes().to_vec())],
        ));
      }
    }
    ops.push(Operation::new("Q", vec![]));
  }
  ops
}

pub struct Output {
  pub bytes: Vec<u8>,
  pub warnings: Vec<String>,
}
pub fn generate(xml: &str) -> Result<Output, String> {
  let udf = model::parse(xml)?;
  let mut r = Renderer {
    pdf: Document::with_version("1.7"),
    fonts: Fonts::new(),
    images: Dictionary::new(),
    warnings: BTreeSet::new(),
    atoms: 0,
  };
  let width = udf.width - udf.margins[1] - udf.margins[3];
  let headers: Vec<_> = udf
    .elements
    .iter()
    .filter(|e| e.tag == "header")
    .cloned()
    .collect();
  let footers: Vec<_> = udf
    .elements
    .iter()
    .filter(|e| e.tag == "footer")
    .cloned()
    .collect();
  let body: Vec<_> = udf
    .elements
    .iter()
    .filter(|e| e.tag != "header" && e.tag != "footer")
    .cloned()
    .collect();
  let header = r.blocks(&headers, width, 0)?;
  let footer = r.blocks(&footers, width, 0)?;
  if header.iter().chain(&footer).any(|b| b.page_break) {
    return Err("Page breaks in headers or footers are unsupported.".into());
  }
  let header_height: f32 = header.iter().map(|b| b.height).sum::<f32>()
    + if header.is_empty() { 0. } else { 12. };
  let footer_height: f32 = footer.iter().map(|b| b.height).sum::<f32>()
    + if footer.is_empty() { 0. } else { 12. };
  let top = udf.margins[0] + header_height;
  let bottom = udf.height - udf.margins[2] - footer_height;
  if bottom - top < 36. {
    return Err("Headers and footers leave no room for document text.".into());
  }
  let bands = r.blocks(&body, width, 0)?;
  let mut pages: Vec<Vec<Draw>> = vec![vec![]];
  let mut y = top;
  for band in bands {
    if band.height > bottom - top {
      return Err("A table row, image, or text line is taller than the printable page. Reduce its size before exporting.".into());
    }
    if band.page_break {
      pages.push(vec![]);
      y = top;
      continue;
    }
    if y + band.height > bottom + 0.01 {
      pages.push(vec![]);
      y = top;
    }
    if pages.len() > 2000 {
      return Err("PDF exceeds 2000 pages.".into());
    }
    for mut d in band.draw {
      d.offset(udf.margins[3], y);
      pages.last_mut().unwrap().push(d);
    }
    y += band.height;
  }
  for page in &mut pages {
    for (bands, start) in [(&header, udf.margins[0]), (&footer, bottom + 12.)] {
      let mut y = start;
      for band in bands {
        for mut d in band.draw.clone() {
          d.offset(udf.margins[3], y);
          page.push(d);
        }
        y += band.height;
      }
    }
  }
  let fonts = r.fonts.embed(&mut r.pdf)?;
  let resources = r
    .pdf
    .add_object(dictionary! {"Font" => fonts, "XObject" => r.images});
  let pages_id = r.pdf.new_object_id();
  let mut kids = vec![];
  for page in pages {
    let content = Content {
      operations: operations(&page, udf.height),
    }
    .encode()
    .map_err(|e| e.to_string())?;
    let contents = r.pdf.add_object(Stream::new(dictionary! {}, content));
    let id = r.pdf.add_object(
      dictionary! {"Type"=>"Page", "Parent"=>pages_id, "Contents"=>contents},
    );
    kids.push(Object::Reference(id));
  }
  r.pdf.objects.insert(pages_id,dictionary! {"Type"=>"Pages", "Count"=>kids.len() as i64, "Kids"=>kids,"Resources"=>resources,"MediaBox"=>vec![Object::Integer(0),Object::Integer(0),Object::Real(udf.width),Object::Real(udf.height)]}.into());
  let catalog = r
    .pdf
    .add_object(dictionary! {"Type"=>"Catalog","Pages"=>pages_id});
  r.pdf.trailer.set("Root", catalog);
  r.pdf.compress();
  let mut bytes = vec![];
  r.pdf.save_to(&mut bytes).map_err(|e| e.to_string())?;
  r.warnings.extend(r.fonts.warnings);
  Ok(Output {
    bytes,
    warnings: r.warnings.into_iter().collect(),
  })
}

#[cfg(test)]
mod tests {
  use super::*;

  const FORMATTED: &str = include_str!("../../../test/fixtures/formatted.xml");
  /// Printable width of the default A4 page, in points.
  const PRINTABLE: f32 = 210. * 72. / 25.4 - 2. * 42.52;

  /// A minimal template: `text` is the shared character pool that `elements`
  /// refers to by offset.
  fn template(text: &str, elements: &str) -> String {
    format!(
      "<template><content><![CDATA[{text}]]></content>\
       <elements>{elements}</elements></template>"
    )
  }

  fn export(xml: &str) -> Output {
    generate(xml).expect("export succeeds")
  }

  fn failure(xml: &str) -> String {
    match generate(xml) {
      Err(message) => message,
      Ok(output) =>
        panic!("expected a failure, got {} bytes", output.bytes.len()),
    }
  }

  fn read(bytes: &[u8]) -> Document {
    Document::load_mem(bytes).expect("valid PDF")
  }

  fn pages(bytes: &[u8]) -> usize {
    read(bytes).get_pages().len()
  }

  /// Every page's drawing operators, in page order.
  fn operators(bytes: &[u8]) -> Vec<Operation> {
    let doc = read(bytes);
    doc
      .get_pages()
      .values()
      .flat_map(|id| {
        Content::decode(&doc.get_page_content(*id))
          .expect("decodable page content")
          .operations
      })
      .collect()
  }

  fn count(bytes: &[u8], operator: &str) -> usize {
    operators(bytes)
      .iter()
      .filter(|op| op.operator == operator)
      .count()
  }

  /// Operands at `index` of every `operator`, over all pages.
  fn operands(bytes: &[u8], operator: &str, index: usize) -> Vec<f32> {
    operators(bytes)
      .iter()
      .filter(|op| op.operator == operator)
      .filter_map(|op| op.operands.get(index)?.as_float().ok())
      .collect()
  }

  /// Where each glyph of the first page sits, in drawing order.
  fn glyphs(bytes: &[u8]) -> Vec<(f32, f32)> {
    let doc = read(bytes);
    let id = *doc.get_pages().values().next().expect("a page");
    Content::decode(&doc.get_page_content(id))
      .expect("decodable page content")
      .operations
      .iter()
      .filter(|op| op.operator == "Tm")
      .filter_map(|op| {
        Some((
          op.operands.get(4)?.as_float().ok()?,
          op.operands.get(5)?.as_float().ok()?,
        ))
      })
      .collect()
  }

  /// The first page's text baselines, top first.
  fn baselines(bytes: &[u8]) -> Vec<i64> {
    let mut result: Vec<i64> = glyphs(bytes)
      .iter()
      .map(|(_, y)| y.round() as i64)
      .collect();
    result.sort_unstable();
    result.dedup();
    result.reverse();
    result
  }

  /// The page box a reader will use, in points.
  fn page_size(bytes: &[u8]) -> (f32, f32) {
    let doc = read(bytes);
    let tree = doc
      .catalog()
      .expect("catalog")
      .get(b"Pages")
      .and_then(Object::as_reference)
      .expect("page tree");
    let media = doc
      .get_dictionary(tree)
      .expect("page tree")
      .get(b"MediaBox")
      .and_then(Object::as_array)
      .expect("media box");
    (
      media[2].as_float().expect("width"),
      media[3].as_float().expect("height"),
    )
  }

  /// A solid translucent PNG, base64 encoded the way a UDF carries one.
  fn png(width: u32, height: u32) -> String {
    let image = image::RgbaImage::from_pixel(
      width,
      height,
      image::Rgba([200, 30, 30, 128]),
    );
    let mut bytes = vec![];
    image
      .write_to(
        &mut std::io::Cursor::new(&mut bytes),
        image::ImageFormat::Png,
      )
      .expect("encodes");
    base64::engine::general_purpose::STANDARD.encode(bytes)
  }

  #[test]
  fn renders_a_formatted_document() {
    let output = export(FORMATTED);
    assert!(output.bytes.starts_with(b"%PDF-1.7"));
    assert!(output.warnings.is_empty(), "{:?}", output.warnings);
    assert_eq!(pages(&output.bytes), 1);
    // Every visible character of the fixture, and the table's fills and rules.
    assert_eq!(count(&output.bytes, "Tj"), 71);
    assert!(count(&output.bytes, "re") > 0);
  }

  #[test]
  fn wraps_long_paragraphs_at_spaces() {
    let text = "kelime ".repeat(120);
    let elements =
      "<paragraph><content startOffset=\"0\" length=\"840\"/></paragraph>";
    let output = export(&template(&text, elements));
    assert_eq!(pages(&output.bytes), 1);
    let placed = glyphs(&output.bytes);
    let lines = baselines(&output.bytes);
    assert!(lines.len() > 5, "wrapped into {} lines", lines.len());
    for line in &lines {
      let start = placed
        .iter()
        .find(|(_, y)| y.round() as i64 == *line)
        .expect("a glyph on the line")
        .0;
      assert!(
        (start - 42.52).abs() < 0.01,
        "every line starts at the margin"
      );
    }
  }

  #[test]
  fn aligns_paragraphs_within_the_margins() {
    let start = |alignment: &str| {
      let elements = format!(
        "<paragraph Alignment=\"{alignment}\">\
         <content startOffset=\"0\" length=\"10\"/></paragraph>"
      );
      glyphs(&export(&template("Kısa satır", &elements)).bytes)[0].0
    };
    assert!(
      (start("0") - 42.52).abs() < 0.01,
      "left starts at the margin"
    );
    assert!(start("1") > start("0"), "centred text is pushed right");
    assert!(
      start("2") > start("1"),
      "right aligned text sits furthest right"
    );
  }

  #[test]
  fn justifies_every_line_but_the_last() {
    let text = "kelime ".repeat(40);
    let end = |alignment: &str| {
      let elements = format!(
        "<paragraph Alignment=\"{alignment}\">\
         <content startOffset=\"0\" length=\"280\"/></paragraph>"
      );
      let bytes = export(&template(&text, &elements)).bytes;
      let placed = glyphs(&bytes);
      let top = placed.iter().map(|&(_, y)| y).fold(f32::MIN, f32::max);
      placed
        .iter()
        .filter(|&&(_, y)| (y - top).abs() < 0.5)
        .map(|&(x, _)| x)
        .fold(f32::MIN, f32::max)
    };
    assert!(end("3") > end("0"), "justified lines reach further right");
  }

  #[test]
  fn indents_and_spaces_paragraphs() {
    let plain =
      "<paragraph><content startOffset=\"0\" length=\"3\"/></paragraph>";
    let moved = "<paragraph LeftIndent=\"40\" SpaceAbove=\"30\">\
                 <content startOffset=\"0\" length=\"3\"/></paragraph>";
    let plain = glyphs(&export(&template("abc", plain)).bytes)[0];
    let moved = glyphs(&export(&template("abc", moved)).bytes)[0];
    assert!((moved.0 - plain.0 - 40.).abs() < 0.01, "indented by 40pt");
    assert!(
      (plain.1 - moved.1 - 30.).abs() < 0.01,
      "pushed down by 30pt"
    );
  }

  #[test]
  fn shifts_superscript_and_subscript_off_the_baseline() {
    let elements = "<paragraph><content startOffset=\"0\" length=\"1\"/>\
                    <content startOffset=\"1\" length=\"1\" superscript=\"true\"/>\
                    <content startOffset=\"2\" length=\"1\" subscript=\"true\"/></paragraph>";
    let output = export(&template("abc", elements));
    let baselines = baselines(&output.bytes);
    assert_eq!(baselines.len(), 3, "three distinct baselines");
    let [raised, normal, lowered] = baselines[..] else {
      unreachable!("three baselines")
    };
    assert!(raised > normal && normal > lowered);
  }

  #[test]
  fn draws_decorations_behind_and_through_text() {
    let elements = "<paragraph><content startOffset=\"0\" length=\"4\" \
                    background=\"16776960\" underline=\"true\" \
                    strikeThrough=\"true\"/></paragraph>";
    let output = export(&template("abcd", elements));
    // A background, an underline, and a strikethrough for each of four glyphs.
    assert_eq!(count(&output.bytes, "re"), 12);
    assert_eq!(count(&output.bytes, "Tj"), 4);
  }

  #[test]
  fn expands_tabs_and_spaces() {
    let spaced = "<paragraph><content startOffset=\"0\" length=\"1\"/><tab/>\
                  <space/><field/><content startOffset=\"1\" length=\"1\"/></paragraph>";
    let tight = "<paragraph><content startOffset=\"0\" length=\"1\"/>\
                 <content startOffset=\"1\" length=\"1\"/></paragraph>";
    let spaced = export(&template("ab", spaced));
    let tight = export(&template("ab", tight));
    // The tab advances without drawing, an empty field contributes nothing,
    // and the space draws a glyph of its own.
    assert_eq!(count(&spaced.bytes, "Tj"), 3);
    assert_eq!(count(&tight.bytes, "Tj"), 2);
    let last = |bytes: &[u8]| glyphs(bytes).last().expect("a glyph").0;
    assert!(last(&spaced.bytes) > last(&tight.bytes) + 10.);
  }

  #[test]
  fn resolves_inherited_named_styles() {
    let xml = "<template><content><![CDATA[ab]]></content>\
               <styles><style name=\"base\" size=\"30\"/>\
               <style name=\"child\" resolver=\"base\" bold=\"true\"/></styles>\
               <elements><paragraph resolver=\"child\">\
               <content startOffset=\"0\" length=\"2\"/></paragraph></elements></template>";
    let output = export(xml);
    assert_eq!(operands(&output.bytes, "Tf", 1), vec![30., 30.]);
  }

  #[test]
  fn splits_long_documents_across_pages() {
    let text = "Satır\n".repeat(200);
    let xml =
      format!("<template><content><![CDATA[{text}]]></content></template>");
    assert!(pages(&export(&xml).bytes) > 1);
  }

  #[test]
  fn starts_a_new_page_at_an_explicit_break() {
    let elements = "<paragraph><content startOffset=\"0\" length=\"1\"/></paragraph>\
                    <page-break/>\
                    <paragraph><content startOffset=\"1\" length=\"1\"/></paragraph>";
    assert_eq!(pages(&export(&template("AB", elements)).bytes), 2);
  }

  #[test]
  fn repeats_headers_and_footers_on_every_page() {
    let one =
      "<paragraph><content startOffset=\"0\" length=\"1\"/></paragraph>";
    let body = format!("{one}<page-break/>{one}");
    let plain = export(&template("A", &body));
    let decorated = export(&template(
      "A",
      &format!("<header>{one}</header><footer>{one}</footer>{body}"),
    ));
    assert_eq!(pages(&plain.bytes), 2);
    assert_eq!(pages(&decorated.bytes), 2);
    // One header and one footer glyph added to each of the two pages.
    assert_eq!(count(&decorated.bytes, "Tj"), count(&plain.bytes, "Tj") + 4);
  }

  #[test]
  fn lays_out_bare_content_as_a_paragraph() {
    let output =
      export(&template("ab", "<content startOffset=\"0\" length=\"2\"/>"));
    assert_eq!(count(&output.bytes, "Tj"), 2);
  }

  #[test]
  fn applies_the_page_format_and_orientation() {
    let letter = "<template><content><![CDATA[A]]></content>\
                  <properties><pageFormat mediaSizeName=\"5\"/></properties></template>";
    let (width, height) = page_size(&export(letter).bytes);
    assert!((width - 215.9 * 72. / 25.4).abs() < 0.01, "US Letter width");
    assert!(height > width, "portrait by default");

    let landscape = letter.replace(
      "mediaSizeName=\"5\"",
      "mediaSizeName=\"5\" paperOrientation=\"0\"",
    );
    let (width, height) = page_size(&export(&landscape).bytes);
    assert!(width > height, "landscape swaps the page box");
  }

  #[test]
  fn draws_embedded_images() {
    let elements = format!(
      "<paragraph><image width=\"80\" height=\"40\" imageData=\"{}\"/></paragraph>",
      png(40, 20)
    );
    let output = export(&template("", &elements));
    assert!(output.warnings.is_empty(), "{:?}", output.warnings);
    assert_eq!(count(&output.bytes, "Do"), 1);
    assert_eq!(operands(&output.bytes, "cm", 0), vec![80.]);
  }

  #[test]
  fn scales_a_wide_image_into_the_column() {
    let elements = format!(
      "<paragraph><image width=\"2000\" height=\"1000\" imageData=\"{}\"/></paragraph>",
      png(40, 20)
    );
    let output = export(&template("", &elements));
    let [width] = operands(&output.bytes, "cm", 0)[..] else {
      unreachable!("one image")
    };
    assert!((width - PRINTABLE).abs() < 0.01, "{width} fills the column");
  }

  #[test]
  fn rejects_unreadable_images() {
    let broken =
      "<paragraph><image imageData=\"not base64 at all\"/></paragraph>";
    assert!(failure(&template("", broken)).contains("Invalid UDF image data"));
    let encoded =
      base64::engine::general_purpose::STANDARD.encode("not an image");
    let garbage =
      format!("<paragraph><image imageData=\"{encoded}\"/></paragraph>");
    assert!(
      failure(&template("", &garbage)).contains("Cannot decode UDF image")
    );
  }

  #[test]
  fn lays_out_tables_with_spans_and_fills() {
    let table = "<table columnSpans=\"100,100,100\"><row>\
                 <cell colspan=\"2\" fillColor=\"15790320\">\
                 <paragraph><content startOffset=\"0\" length=\"1\"/></paragraph></cell>\
                 <cell><paragraph><content startOffset=\"1\" length=\"1\"/></paragraph></cell>\
                 </row></table>";
    let output = export(&template("ab", table));
    assert_eq!(count(&output.bytes, "Tj"), 2);
    let spanned = glyphs(&output.bytes)[0].0;
    let single = glyphs(&output.bytes)[1].0;
    // The second cell starts after two of the three columns.
    assert!((single - spanned - 2. * PRINTABLE / 3.).abs() < 0.01);
  }

  #[test]
  fn divides_a_table_equally_without_column_spans() {
    let table = "<table><row>\
                 <cell><paragraph><content startOffset=\"0\" length=\"1\"/></paragraph></cell>\
                 <cell><paragraph><content startOffset=\"1\" length=\"1\"/></paragraph></cell>\
                 </row></table>";
    let output = export(&template("ab", table));
    let placed = glyphs(&output.bytes);
    assert!((placed[1].0 - placed[0].0 - PRINTABLE / 2.).abs() < 0.01);
  }

  #[test]
  fn draws_table_borders_only_where_asked() {
    let cells =
      "<row><cell><paragraph><content startOffset=\"0\" length=\"1\"/>\
                 </paragraph></cell></row>";
    let rules = |border: &str| {
      let table = format!("<table border=\"{border}\">{cells}</table>");
      count(&export(&template("a", &table)).bytes, "re")
    };
    assert_eq!(rules("borderNone"), 0, "no rules at all");
    assert_eq!(rules(""), 4, "all four cell edges by default");
    assert_eq!(rules("borderTable"), 4, "an outline around the table only");
  }

  #[test]
  fn skips_a_table_without_cells() {
    let output = export(&template("a", "<table></table>"));
    assert_eq!(pages(&output.bytes), 1);
    assert_eq!(count(&output.bytes, "Tj"), 0);
  }

  #[test]
  fn warns_about_formatting_it_cannot_reproduce() {
    let warnings =
      |elements: &str| export(&template("a", elements)).warnings.join(" ");
    let content = "<content startOffset=\"0\" length=\"1\"/>";
    assert!(warnings(&format!(
      "<paragraph Bulleted=\"true\">{content}</paragraph>"
    ))
    .contains("bullets"));
    assert!(warnings(&format!(
      "<paragraph Numbered=\"true\">{content}</paragraph>"
    ))
    .contains("numbering"));
    assert!(warnings(&format!(
      "<paragraph TabSet=\"120\">{content}</paragraph>"
    ))
    .contains("tab stops"));
    assert!(warnings(&format!(
      "<paragraph><unknown>{content}</unknown></paragraph>"
    ))
    .contains("Unsupported inline element 'unknown'"));
    assert!(warnings(&format!(
      "<unknown><paragraph>{content}</paragraph></unknown>"
    ))
    .contains("Unsupported block 'unknown'"));
    assert!(warnings(&format!(
      "<table><row><cell borderStyle=\"dashed\"><paragraph>{content}</paragraph></cell></row></table>"
    ))
    .contains("solid lines"));
  }

  #[test]
  fn warns_when_a_font_is_substituted() {
    let elements = "<paragraph family=\"Kesinlikle Olmayan Yazı Tipi\">\
                    <content startOffset=\"0\" length=\"1\"/></paragraph>";
    let output = export(&template("a", elements));
    assert!(
      output
        .warnings
        .iter()
        .any(|w| w.contains("system font was substituted")),
      "{:?}",
      output.warnings
    );
  }

  #[test]
  fn rejects_malformed_input() {
    assert!(failure("<template>").contains("Invalid UDF XML"));
    assert!(
      failure("<other><content/></other>").contains("Expected a UDF template")
    );
    assert!(failure("<template><elements/></template>").contains("no content"));
    let elements =
      "<paragraph><content startOffset=\"0\" length=\"9\"/></paragraph>";
    assert!(failure(&template("A", elements)).contains("exceeds content"));
    let elements =
      "<paragraph><content startOffset=\"x\" length=\"1\"/></paragraph>";
    assert!(
      failure(&template("A", elements)).contains("Invalid UDF text offset")
    );
    assert!(failure("<template><content>&#127;</content></template>")
      .contains("Unsupported control character"));
  }

  #[test]
  fn rejects_unresolvable_styles_and_deep_nesting() {
    let cyclic = "<template><content><![CDATA[a]]></content>\
                  <styles><style name=\"a\" resolver=\"b\"/>\
                  <style name=\"b\" resolver=\"a\"/></styles>\
                  <elements><paragraph resolver=\"a\"/></elements></template>";
    assert!(failure(cyclic).contains("Cyclic UDF style reference"));
    let deep = format!("{}{}", "<x>".repeat(102), "</x>".repeat(102));
    assert!(failure(&template("a", &deep)).contains("nesting exceeds 100"));
  }

  #[test]
  fn rejects_layouts_that_leave_no_room() {
    let margins = "<template><content><![CDATA[a]]></content><properties>\
                   <pageFormat leftMargin=\"290\" rightMargin=\"290\"/>\
                   </properties></template>";
    assert!(failure(margins).contains("margins leave too little space"));

    let squeezed = "<paragraph LeftIndent=\"300\" RightIndent=\"300\">\
                    <content startOffset=\"0\" length=\"1\"/></paragraph>";
    assert!(failure(&template("a", squeezed)).contains("indents leave no room"));

    let oversized =
      "<paragraph LeftIndent=\"250\" RightIndent=\"250\" size=\"40\">\
                     <content startOffset=\"0\" length=\"1\"/></paragraph>";
    assert!(
      failure(&template("a", oversized)).contains("wider than the paragraph")
    );

    let tall = "<paragraph size=\"500\" LineSpacing=\"1\">\
                <content startOffset=\"0\" length=\"1\"/></paragraph>";
    assert!(
      failure(&template("a", tall)).contains("taller than the printable page")
    );
  }

  #[test]
  fn rejects_headers_that_crowd_out_the_body() {
    let text = "\n".repeat(60);
    let header = "<header><paragraph>\
                  <content startOffset=\"0\" length=\"60\"/></paragraph></header>";
    assert!(
      failure(&template(&text, header)).contains("no room for document text")
    );
    assert!(failure(&template("a", "<footer><page-break/></footer>"))
      .contains("Page breaks in headers or footers"));
  }

  #[test]
  fn rejects_table_shapes_it_cannot_lay_out() {
    let paragraph =
      "<paragraph><content startOffset=\"0\" length=\"1\"/></paragraph>";
    let table = |inner: String| {
      failure(&template("a", &format!("<table>{inner}</table>")))
    };

    assert!(table(format!("<cell>{paragraph}</cell>"))
      .contains("Unsupported table structure"));
    assert!(table(format!("<row>{paragraph}</row>"))
      .contains("Unsupported table row structure"));
    assert!(table(format!(
      "<row><cell rowspan=\"2\">{paragraph}</cell></row>"
    ))
    .contains("spanning multiple rows"));
    assert!(table(
      "<row><cell colspan=\"1000\"/><cell colspan=\"1000\"/></row>".into()
    )
    .contains("Too many table columns"));
    assert!(table(format!(
      "<row>{}</row>",
      format!("<cell>{paragraph}</cell>").repeat(60)
    ))
    .contains("too narrow"));
    assert!(table("<row><cell><page-break/></cell></row>".into())
      .contains("Page breaks inside table cells"));
  }

  #[test]
  fn rejects_documents_beyond_the_export_limits() {
    let padding = " ".repeat(32 * 1024 * 1024);
    let huge = format!("<template><!--{padding}--><content/></template>");
    assert!(failure(&huge).contains("32 MB"));

    let text = "a".repeat(500_001);
    let wordy =
      format!("<template><content><![CDATA[{text}]]></content></template>");
    assert!(failure(&wordy).contains("text limit"));

    let unit = "<paragraph><content startOffset=\"0\" length=\"1\"/></paragraph><page-break/>";
    assert!(failure(&template("A", &unit.repeat(2001)))
      .contains("exceeds 2000 pages"));
  }
}
