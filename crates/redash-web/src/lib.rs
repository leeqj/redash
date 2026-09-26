//! ReDash Web Client - GPUI WebAssembly Canvas Engine
//!
//! 100% Rust WASM Canvas runtime bringing GPUI desktop fidelity and performance
//! to the browser without rewriting UI components in JavaScript.

pub mod app;
pub mod gateway;
pub mod input;
pub mod models;
pub mod render;
pub mod theme;

use app::AppState;
use gateway::GatewayClient;
use input::{handle_key_down, handle_mouse_click, UiAction};
use render::render_frame;
use std::cell::RefCell;
use std::rc::Rc;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use web_sys::{console, HtmlCanvasElement, KeyboardEvent, MouseEvent, Window};

#[wasm_bindgen]
pub fn start_web_app(canvas_id: &str) -> Result<(), JsValue> {
    console_error_panic_hook::set_once();
    console::log_1(&"⚡ ReDash GPUI Web WASM Engine Starting...".into());

    let window = web_sys::window().expect("global window not found");
    let document = window.document().expect("document not found");
    let canvas: HtmlCanvasElement = document
        .get_element_by_id(canvas_id)
        .expect("Canvas element not found")
        .dyn_into::<HtmlCanvasElement>()?;

    let ctx = canvas
        .get_context("2d")?
        .expect("Failed to get 2D canvas context")
        .dyn_into::<web_sys::CanvasRenderingContext2d>()?;

    let app_state = Rc::new(RefCell::new(AppState::new()));
    let gateway = Rc::new(RefCell::new(GatewayClient::new()));

    // Auto-resize canvas to fill browser window
    let resize_canvas = {
        let canvas = canvas.clone();
        let window = window.clone();
        Rc::new(move || {
            let width = window.inner_width().unwrap().as_f64().unwrap();
            let height = window.inner_height().unwrap().as_f64().unwrap();
            let dpr = window.device_pixel_ratio();
            canvas.set_width((width * dpr) as u32);
            canvas.set_height((height * dpr) as u32);
            let html_el: &web_sys::HtmlElement = canvas.unchecked_ref();
            let _ = html_el.style().set_property("width", &format!("{}px", width));
            let _ = html_el.style().set_property("height", &format!("{}px", height));
        })
    };

    resize_canvas();
    let on_resize = {
        let resize_canvas = resize_canvas.clone();
        Closure::<dyn FnMut()>::new(move || {
            resize_canvas();
        })
    };
    window.set_onresize(Some(on_resize.as_ref().unchecked_ref()));
    on_resize.forget();

    // Async Fetch Hosts from Server
    {
        let app_state_clone = app_state.clone();
        let gateway_clone = gateway.clone();
        let on_loaded = Rc::new(RefCell::new(move |hosts: Vec<models::HostConfig>| {
            console::log_1(&format!("Loaded {} hosts from gateway", hosts.len()).into());
            let first_host_id = hosts.first().map(|h| h.id.0.clone());
            app_state_clone.borrow_mut().hosts = hosts;

            // Connect live metrics for first host
            if let Some(host_id) = first_host_id {
                let app_state_metrics = app_state_clone.clone();
                let hid = host_id.clone();
                let _ = gateway_clone.borrow_mut().connect_metrics(
                    &host_id,
                    Rc::new(move |metrics| {
                        app_state_metrics
                            .borrow_mut()
                            .update_metrics(hid.clone(), metrics);
                    }),
                );
            }
        }));
        gateway::async_load_hosts(on_loaded);
    }

    // Register Mouse Event Listener on Canvas
    {
        let app_state_clone = app_state.clone();
        let gateway_clone = gateway.clone();
        let window = window.clone();

        let on_mousedown = Closure::<dyn FnMut(_)>::new(move |e: MouseEvent| {
            let x = e.client_x() as f64;
            let y = e.client_y() as f64;
            let width = window.inner_width().unwrap().as_f64().unwrap();
            let height = window.inner_height().unwrap().as_f64().unwrap();

            let mut state = app_state_clone.borrow_mut();
            if let Some(action) = handle_mouse_click(&mut state, x, y, width, height) {
                match action {
                    UiAction::OpenTerminal(host_id) => {
                        let app_state_term = app_state_clone.clone();
                        let app_state_agent = app_state_clone.clone();
                        let _ = gateway_clone.borrow_mut().connect_terminal(
                            &host_id,
                            Rc::new(move |data| {
                                app_state_term.borrow_mut().append_terminal_output(&data);
                            }),
                            Rc::new(move |agent| {
                                app_state_agent.borrow_mut().agent = Some(agent);
                            }),
                        );
                    }
                    UiAction::SaveNewHost => {
                        if let Some(new_host) = state.build_new_host() {
                            state.hosts.push(new_host);
                        }
                    }
                    _ => {}
                }
            }
        });

        canvas.set_onmousedown(Some(on_mousedown.as_ref().unchecked_ref()));
        on_mousedown.forget();
    }

    // Register Keyboard Event Listener on Canvas
    {
        let app_state_clone = app_state.clone();
        let gateway_clone = gateway.clone();

        let on_keydown = Closure::<dyn FnMut(_)>::new(move |e: KeyboardEvent| {
            let key = e.key();
            let is_ctrl = e.ctrl_key() || e.meta_key();

            let mut state = app_state_clone.borrow_mut();
            if let Some(action) = handle_key_down(&mut state, &key, is_ctrl) {
                match action {
                    UiAction::SendTerminalInput(input) => {
                        gateway_clone.borrow().send_terminal_input(&input);
                    }
                    UiAction::SaveNewHost => {
                        if let Some(new_host) = state.build_new_host() {
                            state.hosts.push(new_host);
                        }
                    }
                    _ => {}
                }
            }
        });

        canvas.set_onkeydown(Some(on_keydown.as_ref().unchecked_ref()));
        on_keydown.forget();
    }

    // High Performance 120 FPS Render Loop via requestAnimationFrame
    {
        let f = Rc::new(RefCell::new(None));
        let g = f.clone();

        let app_state_render = app_state.clone();
        let window_render = window.clone();

        *g.borrow_mut() = Some(Closure::<dyn FnMut()>::new(move || {
            let width = window_render.inner_width().unwrap().as_f64().unwrap();
            let height = window_render.inner_height().unwrap().as_f64().unwrap();
            let dpr = window_render.device_pixel_ratio();

            ctx.save();
            ctx.scale(dpr, dpr).unwrap();

            let state = app_state_render.borrow();
            let _ = render_frame(&ctx, &state, width, height);

            ctx.restore();

            request_animation_frame(f.borrow().as_ref().unwrap(), &window_render);
        }));

        request_animation_frame(g.borrow().as_ref().unwrap(), &window);
    }

    console::log_1(&"✨ ReDash GPUI Web Running Successfully on Canvas!".into());
    Ok(())
}

fn request_animation_frame(f: &Closure<dyn FnMut()>, window: &Window) {
    window
        .request_animation_frame(f.as_ref().unchecked_ref())
        .expect("should register `requestAnimationFrame` OK");
}
