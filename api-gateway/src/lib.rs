mod bindings {
    wit_bindgen::generate!({
        path: "../wit",
        world: "api-gateway",
        generate_all,
    });
}

use wstd::http::{Body, Request, Response, StatusCode};

static UI_HTML: &str = include_str!("../ui.html");

#[wstd::http_server]
async fn main(req: Request<Body>) -> anyhow::Result<Response<Body>> {
    match req.uri().path() {
        "/" => serve_html(),
        _ => not_found(),
    }
}

fn serve_html() -> anyhow::Result<Response<Body>> {
    Response::builder()
        .status(StatusCode::OK)
        .header("Content-Type", "text/html")
        .body(UI_HTML.into())
        .map_err(Into::into)
}

fn not_found() -> anyhow::Result<Response<Body>> {
    Response::builder()
        .status(StatusCode::NOT_FOUND)
        .body("Not found\n".into())
        .map_err(Into::into)
}
