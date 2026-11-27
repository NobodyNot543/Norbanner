//! The shell's own fonts, embedded in the executable: Rubik for text and a
//! subset of Material Symbols Rounded for icons. Both are variable fonts;
//! icons animate their FILL and weight axes.

use windows::{
    Win32::Graphics::DirectWrite::{
        DWRITE_FONT_AXIS_TAG, DWRITE_FONT_AXIS_VALUE, DWRITE_FONT_FAMILY_MODEL_TYPOGRAPHIC,
        IDWriteFactory6, IDWriteFontCollection, IDWriteFontCollection2, IDWriteFontSetBuilder1,
        IDWriteInMemoryFontFileLoader, IDWriteTextFormat3,
    },
    core::{HSTRING, Interface, w},
};

static RUBIK: &[u8] = include_bytes!("../../../../assets/fonts/Rubik.ttf");
static SYMBOLS: &[u8] = include_bytes!("../../../../assets/fonts/MaterialSymbolsRounded.ttf");

pub const ICON_FAMILY: &str = "Material Symbols Rounded";

const fn tag(name: &[u8; 4]) -> DWRITE_FONT_AXIS_TAG {
    DWRITE_FONT_AXIS_TAG(u32::from_le_bytes(*name))
}

pub const WEIGHT: DWRITE_FONT_AXIS_TAG = tag(b"wght");
pub const FILL: DWRITE_FONT_AXIS_TAG = tag(b"FILL");

pub struct Fonts {
    /// Rubik and the icons; `None` if they could not be loaded, in which
    /// case text falls back to Segoe UI and icons are missing.
    collection: Option<IDWriteFontCollection2>,
    _loader: Option<IDWriteInMemoryFontFileLoader>,
}

impl Fonts {
    pub fn load(factory: &IDWriteFactory6) -> Self {
        match Self::try_load(factory) {
            Ok((collection, loader)) => Self {
                collection: Some(collection),
                _loader: Some(loader),
            },
            Err(error) => {
                norbanner_shell::log::line(&format!("could not load the shell fonts: {error}"));
                Self {
                    collection: None,
                    _loader: None,
                }
            }
        }
    }

    fn try_load(
        factory: &IDWriteFactory6,
    ) -> windows::core::Result<(IDWriteFontCollection2, IDWriteInMemoryFontFileLoader)> {
        // SAFETY: The font data is 'static, so the in-memory references
        // stay valid for the life of the process.
        unsafe {
            let loader = factory.CreateInMemoryFontFileLoader()?;
            factory.RegisterFontFileLoader(&loader)?;
            let builder: IDWriteFontSetBuilder1 = factory.CreateFontSetBuilder()?.cast()?;
            for data in [RUBIK, SYMBOLS] {
                let file = loader.CreateInMemoryFontFileReference(
                    factory,
                    data.as_ptr().cast(),
                    data.len() as u32,
                    None,
                )?;
                builder.AddFontFile(&file)?;
            }
            let set = builder.CreateFontSet()?;
            let collection = factory
                .CreateFontCollectionFromFontSet(&set, DWRITE_FONT_FAMILY_MODEL_TYPOGRAPHIC)?;
            Ok((collection, loader))
        }
    }

    fn has(&self, family: &str) -> bool {
        let Some(collection) = &self.collection else {
            return false;
        };
        let mut index = 0;
        let mut exists = windows::core::BOOL(0);
        // SAFETY: Out-pointers are valid locals.
        unsafe {
            collection
                .FindFamilyName(&HSTRING::from(family), &mut index, &mut exists)
                .is_ok()
                && exists.as_bool()
        }
    }

    /// A text format for `family` at `size` DIPs with the given axes. Our
    /// own families come from the embedded collection, others from the
    /// system's.
    pub fn format(
        &self,
        factory: &IDWriteFactory6,
        family: &str,
        size: f32,
        axes: &[DWRITE_FONT_AXIS_VALUE],
    ) -> windows::core::Result<IDWriteTextFormat3> {
        let ours = self.has(family);
        let collection: Option<IDWriteFontCollection> = if ours {
            self.collection.as_ref().and_then(|c| c.cast().ok())
        } else {
            None
        };
        let family = if ours || family != ICON_FAMILY {
            HSTRING::from(family)
        } else {
            HSTRING::from("Segoe UI")
        };
        // SAFETY: Plain factory call with valid strings.
        unsafe {
            factory.CreateTextFormat(&family, collection.as_ref(), axes, size.max(1.0), w!(""))
        }
    }
}
