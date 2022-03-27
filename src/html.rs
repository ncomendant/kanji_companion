use wasm_bindgen::JsCast;
use web_sys::{HtmlElement, Document};

use crate::Result;
use crate::error::Error;

pub struct Html {
    pub characters: HtmlElement,
    pub overlay: OverlayHtml,
}

impl Html {
    pub fn new(document: &Document) -> Result<Self> {
        Ok(Html {
            characters: query(document, "#characters")?,
            overlay: OverlayHtml::new(document)?,
        })
    }
}

pub struct OverlayHtml {
    pub wrapper: HtmlElement,
    pub writing: HtmlElement,
    pub readings: HtmlElement,
    pub meaning: HtmlElement,
    pub parents: HtmlElement,
    pub children: HtmlElement,
}

impl OverlayHtml {
    pub fn new(document: &Document) -> Result<Self> {
        Ok(OverlayHtml {
            wrapper: query(&document, "#overlayWrapper")?,
            writing: query(&document, "#overlay .writing")?,
            readings: query(&document, "#overlay .readings")?,
            meaning: query(&document, "#overlay .meaning")?,
            parents: query(&document, "#overlay .parents")?,
            children: query(&document, "#overlay .children")?,
        })
    }
}

fn query<T: JsCast>(document: &Document, selectors: &str) -> Result<T> {
    let el = document.query_selector(selectors)?.ok_or(Error::ElementNotFound)?.unchecked_into();
    Ok(el)
}