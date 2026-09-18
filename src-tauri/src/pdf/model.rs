/*
 * Copyright (c) 1986-2026 Ecmel Ercan
 * See LICENSE in the project root for license information.
 */

use roxmltree::Node;
use std::collections::{BTreeMap, BTreeSet};

pub type Attrs = BTreeMap<String, String>;
#[derive(Clone, Debug)]
pub struct Element {
  pub tag: String,
  pub attrs: Attrs,
  pub text: String,
  pub children: Vec<Element>,
}
impl Element {
  pub fn get(&self, key: &str) -> &str {
    self.attrs.get(key).map(String::as_str).unwrap_or("")
  }
  pub fn num(&self, key: &str, default: f32) -> f32 {
    number(self.get(key), default)
  }
}
pub fn number(value: &str, default: f32) -> f32 {
  value
    .parse::<f32>()
    .ok()
    .filter(|v| v.is_finite() && v.abs() <= 10000.)
    .unwrap_or(default)
}
pub struct Document {
  pub width: f32,
  pub height: f32,
  pub margins: [f32; 4],
  pub elements: Vec<Element>,
}
fn attrs(node: Node<'_, '_>) -> Attrs {
  node
    .attributes()
    .map(|a| (a.name().into(), a.value().into()))
    .collect()
}
fn named(
  name: &str,
  styles: &BTreeMap<String, Attrs>,
  seen: &mut BTreeSet<String>,
) -> Result<Attrs, String> {
  if !seen.insert(name.into()) {
    return Err("Cyclic UDF style reference.".into());
  }
  let Some(style) = styles.get(name) else {
    return Ok(Attrs::new());
  };
  let mut result = if let Some(parent) = style.get("resolver") {
    named(parent, styles, seen)?
  } else {
    Attrs::new()
  };
  result.extend(style.clone());
  Ok(result)
}
const INHERITED: &[&str] = &[
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
fn element(
  node: Node<'_, '_>,
  parent: &Attrs,
  styles: &BTreeMap<String, Attrs>,
  text: &[char],
  depth: usize,
) -> Result<Element, String> {
  if depth > 100 {
    return Err("UDF nesting exceeds 100 levels.".into());
  }
  let mut resolved: Attrs = parent
    .iter()
    .filter(|(k, _)| INHERITED.contains(&k.as_str()))
    .map(|(k, v)| (k.clone(), v.clone()))
    .collect();
  if let Some(name) = node.attribute("resolver") {
    resolved.extend(named(name, styles, &mut BTreeSet::new())?);
  }
  resolved.extend(attrs(node));
  let tag = node.tag_name().name().to_string();
  let content = if matches!(tag.as_str(), "content" | "field" | "space" | "tab")
  {
    if let Some(offset) = node.attribute("startOffset") {
      let start = offset
        .parse::<usize>()
        .map_err(|_| "Invalid UDF text offset.")?;
      let length = node
        .attribute("length")
        .unwrap_or("")
        .parse::<usize>()
        .map_err(|_| "Invalid UDF text length.")?;
      let end = start.checked_add(length).ok_or("Invalid UDF text range.")?;
      text
        .get(start..end)
        .ok_or("UDF text range exceeds content.")?
        .iter()
        .collect()
    } else if tag == "tab" {
      "\t".into()
    } else if tag == "space" {
      " ".into()
    } else {
      String::new()
    }
  } else {
    String::new()
  };
  let children = node
    .children()
    .filter(Node::is_element)
    .map(|n| element(n, &resolved, styles, text, depth + 1))
    .collect::<Result<_, _>>()?;
  Ok(Element {
    tag,
    attrs: resolved,
    text: content,
    children,
  })
}
pub fn parse(xml: &str) -> Result<Document, String> {
  if xml.len() > 32 * 1024 * 1024 {
    return Err("UDF exceeds the 32 MB PDF export limit.".into());
  }
  // roxmltree rejects DTDs by default and never fetches external resources.
  let tree = roxmltree::Document::parse(xml)
    .map_err(|e| format!("Invalid UDF XML: {e}"))?;
  let root = tree.root_element();
  if root.tag_name().name() != "template" {
    return Err("Expected a UDF template.".into());
  }
  let child = |name| root.children().find(|n| n.has_tag_name(name));
  let text: Vec<char> = child("content")
    .ok_or("UDF has no content.")?
    .children()
    .filter_map(|n| n.text())
    .collect::<String>()
    .chars()
    .collect();
  let styles = child("styles")
    .map(|n| {
      n.children()
        .filter(|n| n.has_tag_name("style"))
        .filter_map(|n| n.attribute("name").map(|name| (name.into(), attrs(n))))
        .collect()
    })
    .unwrap_or_default();
  let mut defaults = Attrs::from([
    ("family".into(), "Times New Roman".into()),
    ("size".into(), "12".into()),
  ]);
  defaults.extend(named("default", &styles, &mut BTreeSet::new())?);
  let elements = if let Some(node) =
    child("elements").filter(|n| n.children().any(|n| n.is_element()))
  {
    element(node, &defaults, &styles, &text, 0)?.children
  } else {
    vec![Element {
      tag: "paragraph".into(),
      attrs: defaults.clone(),
      text: String::new(),
      children: vec![Element {
        tag: "content".into(),
        attrs: defaults,
        text: text.iter().collect(),
        children: vec![],
      }],
    }]
  };
  let format = child("properties")
    .and_then(|p| p.children().find(|n| n.has_tag_name("pageFormat")));
  let property = |key| format.and_then(|n| n.attribute(key)).unwrap_or("");
  let (mut width, mut height) = match property("mediaSizeName") {
    "5" => (215.9, 279.4),
    "6" => (215.9, 355.6),
    "11" => (148., 210.),
    _ => (210., 297.),
  };
  if property("paperOrientation") == "0" {
    std::mem::swap(&mut width, &mut height);
  }
  width *= 72. / 25.4;
  height *= 72. / 25.4;
  let margins = ["topMargin", "rightMargin", "bottomMargin", "leftMargin"]
    .map(|key| number(property(key), 42.52).clamp(0., 300.));
  if width - margins[1] - margins[3] < 36.
    || height - margins[0] - margins[2] < 36.
  {
    return Err("UDF margins leave too little space for PDF content.".into());
  }
  Ok(Document {
    width,
    height,
    margins,
    elements,
  })
}
