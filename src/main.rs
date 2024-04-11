pub mod cli;
pub mod model;
pub mod ort_backend;
pub mod yolo_result;
pub mod cls;

use axum::{
    body::Bytes,
    extract::{Multipart, Path, Request},
    http::StatusCode,
    response::Html,
    routing::{get, post},
    BoxError, Router,
    Json,
};
use futures::{Stream, TryStreamExt};
use std::io;
use tokio_util::io::StreamReader;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};
use base64::{Engine as _, engine::general_purpose};
use std::time::SystemTime;
use serde::{Deserialize, Serialize};

#[derive(Deserialize)]
struct ApiArgs {
    title: Option<String>,
    image: String,
    timestamp: i32,
    device_code: Option<String>,
    license_key: Option<String>,
}

#[derive(Serialize)]
struct ApiReturnData {
    en_name: String,
    cn_name: String,
  }

  #[derive(Serialize)]
struct ApiReturn {
    result_code: i32,
    message: String,
    servertime: i32,
    data: ApiReturnData,
  }

#[tokio::main]
async fn main() {
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "example_stream_to_file=debug".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    // save files to a separate directory to not override files in the current directory

    let app = Router::new()
        .route("/", get(show_form).post(accept_form))
        .route("/file/:file_name", post(save_request_body))
        .route("/api/recognition/agricultural", post(agricultural_api));

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000")
        .await
        .unwrap();
    tracing::debug!("listening on {}", listener.local_addr().unwrap());
    axum::serve(listener, app).await.unwrap();
}

// Handler that streams the request body to a file.
//
// POST'ing to `/file/foo.txt` will create a file called `foo.txt`.
async fn save_request_body(
    Path(file_name): Path<String>,
    request: Request,
) -> Result<String, (StatusCode, String)> {
    stream_to_file(&file_name, request.into_body().into_data_stream()).await
}

// Handler that returns HTML for a multipart form.
async fn show_form() -> Html<&'static str> {
    Html(
        r#"
        <!doctype html>
        <html>
            <head>
                <title>Upload something!</title>
            </head>
            <body>
                <form action="/" method="post" enctype="multipart/form-data">
                    <div>
                        <label>
                            Upload file:
                            <input type="file" name="file" multiple>
                        </label>
                    </div>

                    <div>
                        <input type="submit" value="Upload files">
                    </div>
                </form>
            </body>
        </html>
        "#,
    )
}

// Handler that accepts a multipart form upload and streams each field to a file.
async fn accept_form(mut multipart: Multipart) -> Result<String, (StatusCode, String)> {
    let mut rst = String::new();
    while let Ok(Some(field)) = multipart.next_field().await {
        let file_name = if let Some(file_name) = field.file_name() {
            file_name.to_owned()
        } else {
            continue;
        };

        rst.push_str(&stream_to_file(&file_name, field).await?);
    }

    Ok(rst)
}

async fn agricultural_api(Json(args): Json<ApiArgs>) -> Json<ApiReturn> {
    println!("{:?}\n{}\n{:?}\n{:?}", &args.title, args.timestamp, &args.device_code, &args.license_key);
    let (result_code, message, en_name) = match general_purpose::STANDARD.decode(args.image) {
        Ok(bytes) => match image::load(io::Cursor::new(bytes), image::ImageFormat::Jpeg) {
            Ok(img) => match cls::cls(img) {
                Ok(r) => (0, String::from("成功"), String::from(r.split(' ').collect::<Vec<&str>>()[0])),
                Err(e) => (-1, e.to_string(), String::new()),
            },
            Err(e) => (-1, e.to_string(), String::new()),
        },
        Err(e) => (-1, e.to_string(), String::new()),
    };
    let cn_name = match en_name.as_str() {
        "Bean" => String::from("豌豆"),
        "Bitter_Gourd" => String::from("苦瓜"),
        "Bottle_Gourd" => String::from("葫芦"),
        "Brinjal" => String::from ("茄子"),
        "Broccoli" => String::from("西兰花"),
        "Cabbage" => String::from("卷心菜"),
        "Capsicum" => String::from("辣椒"),
        "Carrot" => String::from("胡萝卜"),
        "Cauliflower" => String::from("花椰菜"),
        "Cucumber" => String::from("黄瓜"),
        "Papaya" => String::from("木瓜"),
        "Potato" => String::from("土豆"),
        "Pumpkin" => String::from("南瓜"),
        "Radish" => String::from("萝卜"),
        "Tomato" => String::from("番茄"),
        _ => String::new(),
    };
    let data = ApiReturnData { en_name, cn_name };
    Json(ApiReturn {
        result_code,
        message,
        servertime: match SystemTime::now().duration_since(SystemTime::UNIX_EPOCH) {
            Ok(n) => match n.as_secs().try_into() {
                Ok(ts) => ts,
                Err(_) => 0,
            },
            Err(_) => 0,
        },
        data,
    })
}

// Save a `Stream` to a file
async fn stream_to_file<S, E>(_path: &str, stream: S) -> Result<String, (StatusCode, String)>
where
    S: Stream<Item = Result<Bytes, E>>,
    E: Into<BoxError>,
{

    async {
        // Convert the stream into an `AsyncRead`.
        let body_with_io_error = stream.map_err(|err| io::Error::new(io::ErrorKind::Other, err));
        let body_reader = StreamReader::new(body_with_io_error);
        futures::pin_mut!(body_reader);

        // Create the file. `File` implements `AsyncWrite`.
        let mut buf = Vec::new();

        // Copy the body into the file.
        tokio::io::copy(&mut body_reader, &mut buf).await?;
        let rst = match image::load(io::Cursor::new(buf), image::ImageFormat::Jpeg) {
            Ok(img) => match cls::cls(img) {
                Ok(r) => r,
                Err(e) => e.to_string(),
            },
            Err(e) => e.to_string(),
        };

        Ok::<_, io::Error>(rst)
    }
    .await
    .map_err(|err| (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()))
}