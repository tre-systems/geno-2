use wasm_bindgen::JsCast;
use web_sys as web;

#[inline]
pub fn window_document() -> Option<web::Document> {
    web::window().and_then(|w| w.document())
}

#[inline]
pub fn add_click_listener(
    document: &web::Document,
    element_id: &str,
    mut handler: impl FnMut() + 'static,
) {
    if let Some(el) = document.get_element_by_id(element_id) {
        let closure =
            wasm_bindgen::closure::Closure::wrap(Box::new(move || handler()) as Box<dyn FnMut()>);
        _ = el.add_event_listener_with_callback("click", closure.as_ref().unchecked_ref());
        closure.forget();
    }
}

pub fn sync_canvas_backing_size(canvas: &web::HtmlCanvasElement) {
    if let Some(w) = web::window() {
        let dpr = w
            .device_pixel_ratio()
            .min(crate::constants::MAX_DEVICE_PIXEL_RATIO);
        let el: web::Element = canvas.clone().unchecked_into();
        let rect = el.get_bounding_client_rect();
        // Clamp to the GPU's max texture dimension so a 5K+ display (even after
        // the DPR cap) can't request a surface/texture the GPU will reject.
        let max = crate::constants::MAX_TEXTURE_DIM;
        let w_px = ((rect.width() * dpr) as u32).clamp(1, max);
        let h_px = ((rect.height() * dpr) as u32).clamp(1, max);
        canvas.set_width(w_px);
        canvas.set_height(h_px);
    }
}
