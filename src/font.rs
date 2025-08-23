// SPDX-License-Identifier: MIT OR Apache-2.0
// SPDX-FileCopyrightText: Copyright (c) 2025 Markus Zehnder

//! Font handling and caching.

use ab_glyph::{FontArc, FontRef, FontVec};
use anyhow::{Context, anyhow};
use log::warn;
use once_cell::sync::Lazy;
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

static DEFAULT_TTF_FONT: Lazy<FontArc> = Lazy::new(|| {
    FontArc::new(
        FontRef::try_from_slice(include_bytes!("../fonts/DejaVuSans.ttf"))
            .expect("Failed to load default font"),
    )
});

pub struct FontHandler {
    ttf_path: PathBuf,
    ttf_cache: HashMap<String, FontArc>,
}

impl FontHandler {
    pub fn new(ttf_path: impl Into<PathBuf>) -> Self {
        Self {
            ttf_path: ttf_path.into(),
            ttf_cache: Default::default(),
        }
    }

    pub fn default_font() -> FontArc {
        DEFAULT_TTF_FONT.clone()
    }

    pub fn get_ttf_font_or_default(&mut self, name: &str) -> FontArc {
        self.get_ttf_font(name).unwrap_or_else(|e| {
            warn!("Failed to load font: {e}. Using default");
            FontHandler::default_font()
        })
    }

    pub fn get_ttf_font(&mut self, name: &str) -> anyhow::Result<FontArc> {
        if let Some(font) = self.ttf_cache.get(name) {
            return Ok(font.clone());
        }
        let mut path = self.ttf_path.join(name);
        path.set_extension("ttf");

        if !path.exists() {
            return Err(anyhow!("{name}.ttf not found"));
        }

        let data = fs::read(path).with_context(|| format!("Error reading font {name}.ttf"))?;
        let font = FontArc::new(
            FontVec::try_from_vec(data)
                .with_context(|| format!("Error parsing font {name}.ttf"))?,
        );

        self.ttf_cache.insert(name.to_string(), font.clone());

        Ok(font)
    }

    #[allow(dead_code)]
    pub fn clear(&mut self) {
        self.ttf_cache.clear();
    }
}
