//! Names SurfaceWatch to desktop-kit, the code every mayorana desktop app
//! shares (the update check and the licence), and lends it SurfaceWatch's
//! HTTP client. In the core so the app and the CLI name it the same way.

use std::future::Future;
use std::path::PathBuf;
use std::pin::Pin;

use desktop_kit::http::{Method, Request, Response};

/// First thing in `main`, in the app and the CLI: until then, nothing in
/// desktop-kit records or sends, and there is no licence to read.
pub fn init() {
    desktop_kit::init(desktop_kit::App {
        id: "surfacewatch",
        name: "SurfaceWatch",
        version: env!("CARGO_PKG_VERSION"),
        // SurfaceWatch sends no usage statistics; this only names its folder.
        telemetry_folder: "surfacewatch",
        product: crate::licence::PRODUCT,
        licence_file,
        http,
    });
}

fn http(req: Request) -> Pin<Box<dyn Future<Output = Option<Response>> + Send>> {
    Box::pin(async move {
        let client = reqwest::Client::new();
        let mut request = match req.method {
            Method::Get => client.get(&req.url),
            Method::Post => client.post(&req.url),
        }
        .header(reqwest::header::USER_AGENT, req.user_agent)
        .timeout(req.timeout);
        for (name, value) in req.headers {
            request = request.header(name, value);
        }
        if let Some(json) = req.json {
            request = request
                .header(reqwest::header::CONTENT_TYPE, "application/json")
                .body(json);
        }
        let response = request.send().await.ok()?;
        let status = response.status().as_u16();
        let body = response.text().await.ok()?;
        Some(Response { status, body })
    })
}

/// Where the app keeps api0's last answer about its licence.
fn licence_file() -> Option<PathBuf> {
    Some(crate::store::data_dir().join("licence.json"))
}
