use warp::Filter;

#[tokio::main]
async fn main() {
    let path = std::env::current_dir().unwrap();
    let route = warp::path("").and(warp::fs::dir(path));
    warp::serve(route).run(([127, 0, 0, 1], 8000)).await;
}
