use axum::{
    body::Body,
    extract::{Path, TypedHeader},
    headers::{ContentRange, ContentType, HeaderMapExt, Range},
    http::{header::CONTENT_TYPE, HeaderMap, Request, StatusCode},
    response::{IntoResponse, Response},
    routing::get,
    Router, Server,
};
use ayaka_model::vfs::{error::VfsErrorKind, *};
use std::{
    fmt::Display,
    io::{Read, SeekFrom},
    net::TcpListener,
    ops::Bound,
    sync::OnceLock,
};
use tauri::{
    plugin::{Builder, TauriPlugin},
    AppHandle, Runtime,
};
use tower_http::{
    cors::{Any, CorsLayer},
    trace::TraceLayer,
};

pub(crate) static ROOT_PATH: OnceLock<VfsPath> = OnceLock::new();

#[derive(Debug)]
struct ResolverError(StatusCode, String);

impl Display for ResolverError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "{} {}", self.0.as_u16(), self.1)
    }
}

impl std::error::Error for ResolverError {}

impl IntoResponse for ResolverError {
    fn into_response(self) -> Response {
        (self.0, self.1).into_response()
    }
}

impl From<VfsError> for ResolverError {
    fn from(err: VfsError) -> Self {
        let msg = err.to_string();
        let code = match err.kind() {
            VfsErrorKind::IoError(_) | VfsErrorKind::Other(_) => StatusCode::INTERNAL_SERVER_ERROR,
            VfsErrorKind::FileNotFound => StatusCode::NOT_FOUND,
            VfsErrorKind::InvalidPath => StatusCode::BAD_REQUEST,
            VfsErrorKind::DirectoryExists | VfsErrorKind::FileExists => StatusCode::CONFLICT,
            VfsErrorKind::NotSupported => StatusCode::NOT_IMPLEMENTED,
        };
        Self(code, msg)
    }
}

impl From<std::io::Error> for ResolverError {
    fn from(err: std::io::Error) -> Self {
        let err: VfsError = err.into();
        Self::from(err)
    }
}

struct RangeNotSatisfiableError;

impl From<RangeNotSatisfiableError> for ResolverError {
    fn from(_: RangeNotSatisfiableError) -> Self {
        Self(StatusCode::RANGE_NOT_SATISFIABLE, String::new())
    }
}

fn get_first_range(range: Range, length: u64) -> Option<(u64, u64)> {
    let mut iter = range.iter();
    let (start, end) = iter.next()?;
    // We don't support multiple ranges.
    if let Some(_) = iter.next() {
        return None;
    }
    let start = match start {
        Bound::Included(i) => i,
        Bound::Excluded(i) => i - 1,
        Bound::Unbounded => 0,
    };
    let end = match end {
        Bound::Included(i) => i + 1,
        Bound::Excluded(i) => i,
        Bound::Unbounded => length,
    };
    if end > length {
        None
    } else {
        Some((start, end))
    }
}

async fn fs_resolver(
    Path(path): Path<String>,
    range: Option<TypedHeader<Range>>,
) -> Result<impl IntoResponse, ResolverError> {
    let path = ROOT_PATH.get().expect("cannot get ROOT_PATH").join(path)?;
    let mime = mime_guess::from_path(path.as_str()).first_or_octet_stream();
    let mut header_map = HeaderMap::new();
    header_map.typed_insert(ContentType::from(mime));
    let mut file = path.open_file()?;
    if let Some(TypedHeader(range)) = range {
        let length = path.metadata()?.len;
        let (start, end) = get_first_range(range, length).ok_or(RangeNotSatisfiableError)?;
        let read_length = end - start;
        let mut buffer = vec![0; read_length as usize];
        file.seek(SeekFrom::Start(start))?;
        file.read_exact(&mut buffer)?;
        header_map.typed_insert(ContentRange::bytes(start..end, length).unwrap());
        Ok((StatusCode::PARTIAL_CONTENT, header_map, buffer))
    } else {
        let mut buffer = vec![];
        file.read_to_end(&mut buffer)?;
        Ok((StatusCode::OK, header_map, buffer))
    }
}

async fn resolver<R: Runtime>(app: AppHandle<R>, req: Request<Body>) -> impl IntoResponse {
    if let Some(asset) = app.asset_resolver().get(req.uri().path().to_string()) {
        Ok(([(CONTENT_TYPE, asset.mime_type)], asset.bytes))
    } else {
        Err(StatusCode::NOT_FOUND)
    }
}

pub fn init<R: Runtime>(listener: TcpListener) -> TauriPlugin<R> {
    Builder::new("asset_resolver")
        .setup(move |app| {
            let app = app.clone();
            tauri::async_runtime::spawn(async {
                let app = Router::new()
                    .route("/fs/*path", get(fs_resolver))
                    .fallback(move |req| resolver(app, req))
                    .layer(
                        TraceLayer::new_for_http()
                            .on_request(())
                            .on_response(())
                            .on_body_chunk(())
                            .on_eos(()),
                    )
                    .layer(CorsLayer::new().allow_methods(Any).allow_origin(Any));
                Server::from_tcp(listener)
                    .expect("cannot create server")
                    .serve(app.into_make_service())
                    .await
                    .expect("cannot serve server")
            });
            Ok(())
        })
        .build()
}
