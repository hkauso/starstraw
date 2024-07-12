extern crate starstraw;

use axum::Router;
use starstraw::*;
use std::env::var;

#[tokio::main]
async fn main() {
    dotenv::dotenv().ok(); // load .env

    let port: u16 = match var("PORT") {
        Ok(v) => v.parse::<u16>().unwrap(),
        Err(_) => 8080,
    };

    // create database
    let database = Database::new(Database::env_options(), ServerOptions::truthy()).await;
    database.init().await;

    // create app
    let app = Router::new()
        .nest("/", starstraw::api::routes(database.clone()))
        .fallback(starstraw::api::not_found);

    let listener = tokio::net::TcpListener::bind(format!("127.0.0.1:{port}"))
        .await
        .unwrap();

    println!("Starting server at http://localhost:{port}!");
    axum::serve(listener, app).await.unwrap();
}
