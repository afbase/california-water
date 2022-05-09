use warp::http::header::{HeaderMap, HeaderValue};
use warp::Filter;

#[tokio::main]
async fn main() {
    let mut headers = HeaderMap::new();
    let path = std::env::current_dir().unwrap();
    let route = warp::path("").and(warp::fs::dir(path));
    warp::serve(route)
        .run(([127, 0, 0, 1], 8000))
        .await;
}
