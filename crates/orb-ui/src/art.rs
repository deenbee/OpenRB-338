//! The panel's SVG parts (`assets/ui` and `assets/modern_ui`), embedded by `build.rs`, and the
//! knob designs. The active variant (`classic` or `modern`) selects which set of parts is
//! served by [`part()`] and [`all()`].

use crate::settings::UiVariant;

/// A part's row: name, URI, SVG bytes, bleed, width, height.
pub type PartRow = (&'static str, &'static str, &'static [u8], f32, f32, f32);

include!(concat!(env!("OUT_DIR"), "/parts_gen_classic.rs"));
include!(concat!(env!("OUT_DIR"), "/parts_gen_modern.rs"));

/// The active UI variant. Changed by [`App`] from the settings; defaults to `classic`.
static UI_VARIANT: std::sync::LazyLock<std::sync::Mutex<UiVariant>> =
    std::sync::LazyLock::new(|| std::sync::Mutex::new(UiVariant::default()));

/// How the classic skin's grainy panels are recolored.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum GrainTransform {
    /// The original tone.
    #[default]
    None,
    /// Rotate the hue by `degrees` (lightness and saturation are kept).
    Hue(f32),
    /// Desaturate to a neutral gray (lightness is kept).
    Gray,
    /// Desaturate to an almost-black gray (lightness scaled down).
    Black,
}

/// The active grain transform. Changed by [`App`] from the settings; only the classic variant
/// reads it.
static GRAIN: std::sync::LazyLock<std::sync::Mutex<GrainTransform>> =
    std::sync::LazyLock::new(|| std::sync::Mutex::new(GrainTransform::None));

/// Sets the active UI variant.
pub fn set_ui_variant(variant: UiVariant) {
    *UI_VARIANT.lock().unwrap_or_else(|e| e.into_inner()) = variant;
}

/// Returns the active UI variant.
pub fn ui_variant() -> UiVariant {
    *UI_VARIANT.lock().unwrap_or_else(|e| e.into_inner())
}

/// Sets the classic skin's grain transform.
pub fn set_grain(transform: GrainTransform) {
    *GRAIN.lock().unwrap_or_else(|e| e.into_inner()) = transform;
}

/// Returns the classic skin's grain transform.
pub fn grain() -> GrainTransform {
    *GRAIN.lock().unwrap_or_else(|e| e.into_inner())
}

/// The desk (around the panel) texture pattern.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DeskTexture {
    /// The original fine grain.
    #[default]
    Fine,
    Coarse,
    Brushed,
    Carbon,
    Dots,
    Houndstooth,
    /// No grain: the gradient only.
    Clean,
}

impl DeskTexture {
    /// Every selectable desk texture, in menu order.
    pub const ALL: [DeskTexture; 7] = [
        DeskTexture::Fine,
        DeskTexture::Coarse,
        DeskTexture::Brushed,
        DeskTexture::Carbon,
        DeskTexture::Dots,
        DeskTexture::Houndstooth,
        DeskTexture::Clean,
    ];
}

impl std::fmt::Display for DeskTexture {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            DeskTexture::Fine => "Fine",
            DeskTexture::Coarse => "Coarse",
            DeskTexture::Brushed => "Brushed",
            DeskTexture::Carbon => "Carbon",
            DeskTexture::Dots => "Dots",
            DeskTexture::Houndstooth => "Houndstooth",
            DeskTexture::Clean => "Clean",
        })
    }
}

/// The active desk texture. Changed by [`App`] from the settings.
static DESK_TEXTURE: std::sync::LazyLock<std::sync::Mutex<DeskTexture>> =
    std::sync::LazyLock::new(|| std::sync::Mutex::new(DeskTexture::Fine));

/// Sets the active desk texture.
pub fn set_desk_texture(texture: DeskTexture) {
    *DESK_TEXTURE.lock().unwrap_or_else(|e| e.into_inner()) = texture;
}

/// Returns the active desk texture.
pub fn desk_texture() -> DeskTexture {
    *DESK_TEXTURE.lock().unwrap_or_else(|e| e.into_inner())
}

/// The part name of the active desk texture's tile; `None` for a clean desk (gradient only).
pub fn desk_tile_part() -> Option<&'static str> {
    match desk_texture() {
        DeskTexture::Fine => Some("desk_tile"),
        DeskTexture::Coarse => Some("desk_tile_coarse"),
        DeskTexture::Brushed => Some("desk_tile_brushed"),
        DeskTexture::Carbon => Some("desk_tile_carbon"),
        DeskTexture::Dots => Some("desk_tile_dots"),
        DeskTexture::Houndstooth => Some("desk_tile_houndstooth"),
        DeskTexture::Clean => None,
    }
}

/// The part table for the active variant.
fn parts_table() -> &'static [PartRow] {
    match ui_variant() {
        UiVariant::Classic => PARTS_UI,
        UiVariant::Modern => PARTS_MODERN_UI,
    }
}

/// A part: its URI for egui's loaders, its SVG bytes and its bleed: how far, in panel pixels,
/// the art reaches beyond its control's rectangle on every side (shadows, halos, glows;
/// task-179). The SVG's canvas is the rectangle grown by the bleed.
#[derive(Clone, Copy)]
pub struct Part {
    pub name: &'static str,
    pub uri: &'static str,
    pub svg: &'static [u8],
    pub bleed: f32,
    /// The control's rectangle (panel pixels) the part is drawn for.
    pub w: f32,
    pub h: f32,
}

impl Part {
    fn from_row(row: &PartRow) -> Self {
        let &(name, uri, svg, bleed, w, h) = row;
        Self {
            name,
            uri,
            svg,
            bleed,
            w,
            h,
        }
    }
}

/// Maps a layout part name to its file name in `assets/modern_ui`, when the two
/// differ. Returns `None` when the name is unchanged (lookup `name` as-is).
fn modern_part_name(name: &str) -> Option<&str> {
    match name {
        "bg_mixer_1" | "bg_mixer_2" | "bg_mixer_3" | "bg_mixer_4" => Some("bg_mixer"),
        "bg_pattern_1" | "bg_pattern_2" | "bg_pattern_3" | "bg_pattern_4" => Some("bg_pattern"),
        "wood_left" | "wood_right" => Some("wood"),
        "led_303_on" | "led_303s_on" => Some("led_8_on"),
        "led_303_off" | "led_303s_off" => Some("led_8_off"),
        "sel808_AC" | "sel808_BD" | "sel808_SD" | "sel808_LT" | "sel808_MT" | "sel808_HT"
        | "sel808_RS" | "sel808_CP" | "sel808_CB" | "sel808_CY" | "sel808_OH" | "sel808_CH"
        | "sel808_LC" | "sel808_MC" | "sel808_HC" | "sel808_CL" | "sel808_MA" => Some("sel_22x14"),
        "sel909_AC" | "sel909_RS" | "sel909_CP" | "sel909_CH" | "sel909_OH" | "sel909_CC"
        | "sel909_RC" => Some("sel_21x14"),
        "sel909_BD" | "sel909_SD" | "sel909_LT" | "sel909_MT" | "sel909_HT" => Some("sel_42x14"),
        _ => None,
    }
}

/// The part called `name` (the active variant's table is sorted by name).
pub fn part(name: &str) -> Option<Part> {
    part_for(name, ui_variant())
}

fn part_for(name: &str, variant: UiVariant) -> Option<Part> {
    let table = match variant {
        UiVariant::Classic => PARTS_UI,
        UiVariant::Modern => PARTS_MODERN_UI,
    };
    let lookup_name = if variant == UiVariant::Modern {
        modern_part_name(name).unwrap_or(name)
    } else {
        name
    };
    if let Ok(i) = table.binary_search_by(|p| p.0.cmp(lookup_name)) {
        return Some(Part::from_row(&table[i]));
    }
    // The modern set is still growing. Keep the panel complete for parts that have not yet
    // received a modern drawing by using the matching classic SVG.
    if variant == UiVariant::Modern {
        let i = PARTS_UI.binary_search_by(|p| p.0.cmp(name)).ok()?;
        return Some(Part::from_row(&PARTS_UI[i]));
    }
    None
}

/// The part names asked for that no art has, in this process.
static MISSING: std::sync::Mutex<std::collections::BTreeSet<String>> =
    std::sync::Mutex::new(std::collections::BTreeSet::new());

/// Records that the part `name` is missing; true the first time it is.
pub fn report_missing(name: &str) -> bool {
    let mut m = MISSING.lock().unwrap_or_else(|e| e.into_inner());
    m.insert(name.to_owned())
}

/// The part names drawn as nothing because no art has them, in this process (task-156).
pub fn missing() -> Vec<String> {
    let m = MISSING.lock().unwrap_or_else(|e| e.into_inner());
    m.iter().cloned().collect()
}

/// Every part of the active variant.
pub fn all() -> impl Iterator<Item = Part> {
    parts_table().iter().map(Part::from_row)
}

/// A knob design: the layers are `<name>_body` (static) and `<name>_pointer`, rotated about
/// `(cx, cy)` (assets/ui/README.md). The knobs have no value arc since task-179.
pub struct KnobDesign {
    pub name: &'static str,
    pub w: f32,
    pub h: f32,
    pub cx: f32,
    pub cy: f32,
}

const MODERN_KNOB_22: KnobDesign = KnobDesign {
    name: "knob_22",
    w: 22.0,
    h: 22.0,
    cx: 11.0,
    cy: 11.0,
};

pub const KNOBS: [KnobDesign; 5] = [
    KnobDesign {
        name: "knob_303",
        w: 30.0,
        h: 30.0,
        cx: 15.0,
        cy: 15.0,
    },
    KnobDesign {
        name: "knob_808_red",
        w: 13.0,
        h: 13.0,
        cx: 6.5,
        cy: 6.5,
    },
    KnobDesign {
        name: "knob_808_white",
        w: 13.0,
        h: 13.0,
        cx: 6.5,
        cy: 6.5,
    },
    KnobDesign {
        name: "knob_909",
        w: 12.0,
        h: 12.0,
        cx: 6.0,
        cy: 6.0,
    },
    // The gray sections' knobs: shuffle, the mixer's pan and delay send, the effects'.
    KnobDesign {
        name: "knob_gray",
        w: 18.0,
        h: 19.0,
        cx: 9.0,
        cy: 9.5,
    },
];

/// The design of a knob control, by its layout part name.
pub fn knob_design(part: &str) -> &'static KnobDesign {
    if part == "knob_22" {
        return &MODERN_KNOB_22;
    }
    KNOBS
        .iter()
        .find(|k| k.name == part)
        .unwrap_or_else(|| panic!("no knob design {part}"))
}

/// The window's icon: the `app_icon` part rendered at `size` x `size` pixels.
pub fn app_icon(size: u32) -> eframe::egui::IconData {
    let svg = part("app_icon").expect("app_icon part").svg;
    let tree = resvg::usvg::Tree::from_data(svg, &resvg::usvg::Options::default())
        .expect("app_icon parses");
    let mut pixmap = resvg::tiny_skia::Pixmap::new(size, size).unwrap();
    let scale = size as f32 / tree.size().width();
    resvg::render(
        &tree,
        resvg::tiny_skia::Transform::from_scale(scale, scale),
        &mut pixmap.as_mut(),
    );
    // IconData is straight (not premultiplied) RGBA.
    let rgba = pixmap
        .pixels()
        .iter()
        .flat_map(|p| {
            let c = p.demultiply();
            [c.red(), c.green(), c.blue(), c.alpha()]
        })
        .collect();
    eframe::egui::IconData {
        rgba,
        width: size,
        height: size,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn knob_designs_match_the_art_generator() {
        assert_eq!(KNOBS.len(), orb_ui_art::parts::KNOBS.len());
        for (a, b) in KNOBS.iter().zip(orb_ui_art::parts::KNOBS.iter()) {
            assert_eq!(a.name, b.name);
            assert_eq!((a.w, a.h, a.cx, a.cy), (b.w, b.h, b.cx, a.cy));
            // the pointer turns about the canvas's center: the art centers the knob
            assert_eq!((a.cx * 2.0, a.cy * 2.0), (a.w, a.h), "{}", a.name);
        }
    }

    #[test]
    fn modern_knob_22_design_matches_its_art() {
        let knob = knob_design("knob_22");
        assert_eq!(knob.name, "knob_22");
        assert_eq!((knob.w, knob.h, knob.cx, knob.cy), (22.0, 22.0, 11.0, 11.0));
        assert!(part_for("knob_22_body", UiVariant::Modern).is_some());
        assert!(part_for("knob_22_pointer", UiVariant::Modern).is_some());
    }

    #[test]
    fn classic_parts_are_embedded_and_sorted() {
        let table = PARTS_UI;
        assert!(!table.is_empty());
        assert!(table.windows(2).all(|w| w[0].0 < w[1].0));
        assert!(part("btn_play_lit").is_some());
        assert!(part("panel").is_none());
        assert!(part("nope").is_none());
    }

    #[test]
    fn modern_table_exists() {
        // the modern variant may be empty until assets/modern_ui has SVGs; the table must exist
        let _ = PARTS_MODERN_UI;
    }

    #[test]
    fn modern_parts_resolve_aliases_and_classic_fallbacks() {
        assert_eq!(
            part_for("wood_left", UiVariant::Modern).unwrap().name,
            "wood"
        );
        assert_eq!(
            part_for("led_303_on", UiVariant::Modern).unwrap().name,
            "led_8_on"
        );
        assert_eq!(
            part_for("switch_wave_0", UiVariant::Modern).unwrap().name,
            "switch_wave_0"
        );
        assert_eq!(
            part_for("mod_view_glass", UiVariant::Modern).unwrap().name,
            "mod_view_glass"
        );
    }
}
