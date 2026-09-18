/*
 * Copyright (c) 1986-2026 Ecmel Ercan
 * See LICENSE in the project root for license information.
 */

use fontdb::{Database, Family, Query, Style, Weight};
use lopdf::{dictionary, Dictionary, Document, Object, Stream};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use subsetter::GlyphRemapper;

/// A font family with its bold and italic flags, and the character to render.
type GlyphKey = (String, bool, bool, char);
/// An embedded font's index, the glyph's identifier, and its advance width in
/// em units.
type Glyph = (usize, u16, f32);

struct Font {
  data: Vec<u8>,
  index: u32,
  glyphs: GlyphRemapper,
  unicode: BTreeMap<u16, char>,
}
pub struct Fonts {
  db: Database,
  fonts: Vec<Font>,
  loaded: HashMap<fontdb::ID, usize>,
  cache: HashMap<GlyphKey, Glyph>,
  pub warnings: BTreeSet<String>,
}
impl Fonts {
  pub fn new() -> Self {
    let mut db = Database::new();
    // fontdb has no mobile support: on iOS it would scan the Linux font
    // directories, and on Android it loads nothing.
    #[cfg(target_os = "ios")]
    db.load_fonts_dir("/System/Library/Fonts");
    #[cfg(target_os = "android")]
    {
      db.load_fonts_dir("/system/fonts");
      // Android lacks Times New Roman, fontdb's default serif family.
      db.set_serif_family("Noto Serif");
    }
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    db.load_system_fonts();
    Self {
      db,
      fonts: vec![],
      loaded: HashMap::new(),
      cache: HashMap::new(),
      warnings: BTreeSet::new(),
    }
  }
  pub fn glyph(
    &mut self,
    family: &str,
    bold: bool,
    italic: bool,
    ch: char,
  ) -> Result<Glyph, String> {
    let key = (family.into(), bold, italic, ch);
    if let Some(value) = self.cache.get(&key) {
      return Ok(*value);
    }
    let requested = self.db.query(&Query {
      families: &[Family::Name(family), Family::Serif],
      weight: if bold { Weight::BOLD } else { Weight::NORMAL },
      style: if italic { Style::Italic } else { Style::Normal },
      ..Query::default()
    });
    let supports = |id| {
      self
        .db
        .with_face_data(id, |data, index| {
          ttf_parser::Face::parse(data, index).ok().is_some_and(|f| {
            f.tables().glyf.is_some()
              && f.glyph_index(ch).is_some()
              && !matches!(
                f.permissions(),
                Some(ttf_parser::Permissions::Restricted)
              )
          })
        })
        .unwrap_or(false)
    };
    let id = requested.filter(|id| supports(*id)).or_else(|| self.db.faces().find(|f| supports(f.id)).map(|f| f.id)).ok_or_else(|| format!("No embeddable TrueType font contains U+{:04X} ({ch}). Install a font supporting this character.", ch as u32))?;
    if requested != Some(id)
      || self.db.face(id).is_some_and(|f| {
        !f.families
          .iter()
          .any(|(name, _)| name.eq_ignore_ascii_case(family))
      })
    {
      self.warnings.insert(format!(
        "A system font was substituted for {family} or some of its characters."
      ));
    }
    let font_index = if let Some(index) = self.loaded.get(&id) {
      *index
    } else {
      let (data, index) = self
        .db
        .with_face_data(id, |d, i| (d.to_vec(), i))
        .ok_or("Unable to read font.")?;
      let result = self.fonts.len();
      self.fonts.push(Font {
        data,
        index,
        glyphs: GlyphRemapper::new(),
        unicode: BTreeMap::new(),
      });
      self.loaded.insert(id, result);
      result
    };
    let font = &mut self.fonts[font_index];
    let face = ttf_parser::Face::parse(&font.data, font.index)
      .map_err(|_| "Invalid font.")?;
    let gid = face.glyph_index(ch).ok_or("Missing glyph.")?;
    let width = face.glyph_hor_advance(gid).unwrap_or(0) as f32
      / face.units_per_em() as f32;
    let cid = font.glyphs.remap(gid.0);
    font.unicode.entry(cid).or_insert(ch);
    let result = (font_index, cid, width);
    self.cache.insert(key, result);
    Ok(result)
  }
  pub fn embed(&self, doc: &mut Document) -> Result<Dictionary, String> {
    let mut resources = Dictionary::new();
    for (index, font) in self.fonts.iter().enumerate() {
      let face = ttf_parser::Face::parse(&font.data, font.index)
        .map_err(|_| "Invalid font.")?;
      let scale = 1000. / face.units_per_em() as f32;
      let bytes = subsetter::subset(&font.data, font.index, &font.glyphs)
        .map_err(|e| format!("Font embedding failed: {e}"))?;
      let file = doc.add_object(Stream::new(
        dictionary! { "Length1" => bytes.len() as i64 },
        bytes,
      ));
      let name = format!("VRDTUM+Font{index}");
      let bbox = face.global_bounding_box();
      let descriptor = doc.add_object(dictionary! {
        "Type" => "FontDescriptor", "FontName" => Object::Name(name.as_bytes().to_vec()),
        "Flags" => 4, "FontBBox" => vec![Object::Real(bbox.x_min as f32 * scale), Object::Real(bbox.y_min as f32 * scale), Object::Real(bbox.x_max as f32 * scale), Object::Real(bbox.y_max as f32 * scale)],
        "ItalicAngle" => face.italic_angle(), "Ascent" => face.ascender() as f32 * scale,
        "Descent" => face.descender() as f32 * scale, "CapHeight" => face.capital_height().unwrap_or(face.ascender()) as f32 * scale,
        "StemV" => 80, "FontFile2" => file,
      });
      let mut widths = vec![];
      let mut mappings = vec![];
      for (cid, ch) in &font.unicode {
        let gid = face.glyph_index(*ch).ok_or("Missing embedded glyph.")?;
        widths.push(Object::Integer(*cid as i64));
        widths.push(Object::Array(vec![Object::Real(
          face.glyph_hor_advance(gid).unwrap_or(0) as f32 * scale,
        )]));
        let unicode = ch
          .encode_utf16(&mut [0; 2])
          .iter()
          .map(|v| format!("{v:04X}"))
          .collect::<String>();
        mappings.push(format!("<{cid:04X}> <{unicode}>\n"));
      }
      let mut cmap = String::from("/CIDInit /ProcSet findresource begin\n12 dict begin\nbegincmap\n/CIDSystemInfo << /Registry (Adobe) /Ordering (UCS) /Supplement 0 >> def\n/CMapName /Adobe-Identity-UCS def\n/CMapType 2 def\n1 begincodespacerange\n<0000> <FFFF>\nendcodespacerange\n");
      for chunk in mappings.chunks(100) {
        cmap.push_str(&format!(
          "{} beginbfchar\n{}endbfchar\n",
          chunk.len(),
          chunk.concat()
        ));
      }
      cmap.push_str(
        "endcmap\nCMapName currentdict /CMap defineresource pop\nend\nend\n",
      );
      let unicode =
        doc.add_object(Stream::new(dictionary! {}, cmap.into_bytes()));
      let descendant = doc.add_object(dictionary! {
        "Type" => "Font", "Subtype" => "CIDFontType2", "BaseFont" => Object::Name(name.as_bytes().to_vec()),
        "CIDSystemInfo" => dictionary! { "Registry" => Object::string_literal("Adobe"), "Ordering" => Object::string_literal("Identity"), "Supplement" => 0 },
        "FontDescriptor" => descriptor, "CIDToGIDMap" => "Identity", "W" => widths,
      });
      let type0 = doc.add_object(dictionary! { "Type" => "Font", "Subtype" => "Type0", "BaseFont" => Object::Name(name.into_bytes()), "Encoding" => "Identity-H", "DescendantFonts" => vec![Object::Reference(descendant)], "ToUnicode" => unicode });
      resources.set(format!("F{index}"), type0);
    }
    Ok(resources)
  }
}
