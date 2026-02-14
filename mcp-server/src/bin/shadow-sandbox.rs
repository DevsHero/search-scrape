use axum::{routing::get, Router};
use std::net::SocketAddr;

async fn ok() -> &'static str {
    r#"<!doctype html><html><head><title>OK</title></head><body><h1>OK</h1><p>Sandbox OK</p></body></html>"#
}

async fn challenge_auto() -> &'static str {
    // Contains challenge markers + auto-removes iframe after 4s.
    r#"<!doctype html>
<html>
  <head>
    <title>Verification Auto</title>
    <meta charset="utf-8" />
    <script>
      setTimeout(() => {
        const f = document.querySelector('iframe');
        if (f) f.remove();
        document.body.insertAdjacentHTML('beforeend', '<p id="solved">Solved</p>');
      }, 4000);
    </script>
  </head>
  <body>
    <h1>Verification step required</h1>
    <iframe src="about:blank" title="verification challenge"></iframe>
    <p>Simulated verification gate (auto-resolves)…</p>
  </body>
</html>"#
}

async fn challenge_manual() -> &'static str {
    // Contains challenge markers + requires user click.
    r#"<!doctype html>
<html>
  <head>
    <title>Verification Manual</title>
    <meta charset="utf-8" />
    <script>
      function solve() {
        const f = document.querySelector('iframe');
        if (f) f.remove();
        document.getElementById('btn').remove();
        document.body.insertAdjacentHTML('beforeend', '<p id="solved">Solved</p>');
      }
    </script>
  </head>
  <body>
    <h1>Verification step required</h1>
    <iframe src="about:blank" title="verification challenge"></iframe>
    <button id="btn" onclick="solve()" style="font-size:24px;padding:16px;">I completed the verification step</button>
    <p>Simulated verification gate (manual)…</p>
  </body>
</html>"#
}

#[tokio::main]
async fn main() {
    let app = Router::new()
        .route("/ok", get(ok))
        .route("/challenge_auto", get(challenge_auto))
        .route("/challenge_manual", get(challenge_manual));

    let addr: SocketAddr = "127.0.0.1:8787".parse().expect("addr");
    eprintln!("shadow-sandbox listening on http://{}", addr);
    axum::serve(tokio::net::TcpListener::bind(addr).await.unwrap(), app)
        .await
        .unwrap();
}
