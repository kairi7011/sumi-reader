use std::path::Path;
use objc2::{extern_class, msg_send, ClassType, MainThreadOnly};
use objc2::rc::{Allocated, Retained};
use objc2::runtime::{AnyObject, NSObject};
use objc2_app_kit::{NSImage, NSView};
use objc2_foundation::{NSArray, NSRect, NSSize, NSString, NSURL};

#[link(name = "PDFKit", kind = "framework")]
unsafe extern "C" {}

extern_class!(
    #[unsafe(super(NSObject))]
    #[derive(Debug, PartialEq, Eq, Hash)]
    pub struct PDFDocument;
);
extern_class!(
    #[unsafe(super(NSObject))]
    #[derive(Debug, PartialEq, Eq, Hash)]
    pub struct PDFPage;
);
extern_class!(
    #[unsafe(super(NSObject))]
    #[derive(Debug, PartialEq, Eq, Hash)]
    pub struct PDFSelection;
);
extern_class!(
    #[unsafe(super(NSObject))]
    #[derive(Debug, PartialEq, Eq, Hash)]
    pub struct PDFOutline;
);
extern_class!(
    #[unsafe(super(NSObject))]
    #[derive(Debug, PartialEq, Eq, Hash)]
    pub struct PDFDestination;
);
extern_class!(
    #[unsafe(super(NSView))]
    #[thread_kind = MainThreadOnly]
    #[derive(Debug, PartialEq, Eq, Hash)]
    pub struct PDFView;
);

pub const CROP_BOX: isize = 1;

impl PDFDocument {
    pub fn from_path(path: &Path) -> Option<Retained<Self>> {
        let url = NSURL::fileURLWithPath(&NSString::from_str(path.to_str()?));
        unsafe {
            let allocated: Allocated<Self> = msg_send![Self::class(), alloc];
            msg_send![allocated, initWithURL: &*url]
        }
    }

    pub fn page_count(&self) -> usize { unsafe { msg_send![self, pageCount] } }
    pub fn page(&self, index: usize) -> Option<Retained<PDFPage>> {
        if index >= self.page_count() { return None; }
        unsafe { msg_send![self, pageAtIndex: index] }
    }
    pub fn index_of(&self, page: &PDFPage) -> usize { unsafe { msg_send![self, indexForPage: page] } }
    pub fn is_locked(&self) -> bool { unsafe { msg_send![self, isLocked] } }
    pub fn unlock(&self, password: &str) -> bool { unsafe { msg_send![self, unlockWithPassword: &*NSString::from_str(password)] } }
    pub fn allows_copying(&self) -> bool { unsafe { msg_send![self, allowsCopying] } }

    pub fn title(&self) -> Option<String> {
        unsafe {
            let attributes: Option<Retained<AnyObject>> = msg_send![self, documentAttributes];
            let value: Option<Retained<AnyObject>> = msg_send![attributes.as_ref()?, objectForKey: &*NSString::from_str("Title")];
            let title = value?.downcast::<NSString>().ok()?.to_string();
            let title = title.trim();
            (!title.is_empty()).then(|| title.to_owned())
        }
    }

    pub fn outline(&self) -> Vec<(String, usize)> {
        fn visit(document: &PDFDocument, item: &PDFOutline, depth: usize, output: &mut Vec<(String, usize)>) {
            if depth > 12 || output.len() >= 1000 { return; }
            unsafe {
                let label: Option<Retained<NSString>> = msg_send![item, label];
                let destination: Option<Retained<PDFDestination>> = msg_send![item, destination];
                if let (Some(label), Some(destination)) = (label, destination) {
                    let page: Option<Retained<PDFPage>> = msg_send![&*destination, page];
                    if let Some(page) = page {
                        let index = document.index_of(&page);
                        if index < document.page_count() { output.push((format!("{}{}", "  ".repeat(depth.saturating_sub(1)), label), index + 1)); }
                    }
                }
                let count: usize = msg_send![item, numberOfChildren];
                for index in 0..count.min(1000) {
                    let child: Option<Retained<PDFOutline>> = msg_send![item, childAtIndex: index];
                    if let Some(child) = child { visit(document, &child, depth + 1, output); }
                    if output.len() >= 1000 { break; }
                }
            }
        }
        let mut output = Vec::new();
        let root: Option<Retained<PDFOutline>> = unsafe { msg_send![self, outlineRoot] };
        if let Some(root) = root { visit(self, &root, 0, &mut output); }
        output
    }
}

impl PDFPage {
    pub fn text(&self) -> String {
        let value: Option<Retained<NSString>> = unsafe { msg_send![self, string] };
        value.map(|text| text.to_string()).unwrap_or_default()
    }
    pub fn bounds(&self) -> NSRect { unsafe { msg_send![self, boundsForBox: CROP_BOX] } }
    pub fn thumbnail(&self, size: NSSize) -> Retained<NSImage> { unsafe { msg_send![self, thumbnailOfSize: size, forBox: CROP_BOX] } }
    pub fn selection_for_rect(&self, rect: NSRect) -> Option<Retained<PDFSelection>> { unsafe { msg_send![self, selectionForRect: rect] } }
}

impl PDFSelection {
    pub fn text(&self) -> String {
        let value: Option<Retained<NSString>> = unsafe { msg_send![self, string] };
        value.map(|text| text.to_string()).unwrap_or_default()
    }
    pub fn pages(&self) -> Retained<NSArray<PDFPage>> { unsafe { msg_send![self, pages] } }
}
