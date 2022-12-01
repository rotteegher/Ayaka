use actix_files::NamedFile;
use actix_web::{
    dev::Service,
    http::header::{ContentType, HeaderValue, ACCESS_CONTROL_ALLOW_ORIGIN},
    web, App, HttpRequest, HttpResponse, HttpServer, Responder,
};
use ayaka_runtime::log;
use std::{path::PathBuf, sync::OnceLock};
use tauri::{
    plugin::{Builder, TauriPlugin},
    AppHandle, Runtime,
};

pub(crate) static ROOT_PATH: OnceLock<PathBuf> = OnceLock::new();

async fn fs_resolver<R: Runtime>(app: AppHandle<R>, req: HttpRequest) -> impl Responder {
    let url = req.uri().path();
    log::debug!("Acquiring {}", url);
    if url.starts_with("/fs/") {
        let path = ROOT_PATH
            .get()
            .unwrap()
            .join(url.strip_prefix("/fs/").unwrap());
        if path.is_file() {
            NamedFile::open_async(&path)
                .await
                .unwrap()
                .into_response(&req)
        } else {
            HttpResponse::NotFound().finish()
        }
    } else if let Some(asset) = app.asset_resolver().get(url.to_string()) {
        HttpResponse::Ok()
            .append_header(ContentType(asset.mime_type.parse().unwrap()))
            .body(asset.bytes)
    } else {
        HttpResponse::NotFound().finish()
    }
}

pub fn init<R: Runtime>(port: u16) -> TauriPlugin<R> {
    Builder::new("asset_resolver")
        .setup(move |app| {
            let app = app.clone();
            std::thread::spawn(move || {
                actix_web::rt::System::new().block_on(async move {
                    HttpServer::new(move || {
                        let app = app.clone();
                        App::new()
                            .default_service(web::to(move |req| fs_resolver(app.clone(), req)))
                            .wrap_fn(|req, srv| {
                                let fut = srv.call(req);
                                async {
                                    let mut res = fut.await?;
                                    res.headers_mut().insert(
                                        ACCESS_CONTROL_ALLOW_ORIGIN,
                                        HeaderValue::from_static("*"),
                                    );
                                    Ok(res)
                                }
                            })
                    })
                    .bind(("127.0.0.1", port))
                    .unwrap()
                    .run()
                    .await
                })
            });
            Ok(())
        })
        .build()
}