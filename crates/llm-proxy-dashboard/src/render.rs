use axum::{
    http::StatusCode,
    response::{Html, IntoResponse, Response},
};

pub(crate) fn page(title: &str, body: &str) -> String {
    format!(
        r#"<!doctype html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <title>{}</title>
</head>
<body>
  <main>{}</main>
</body>
</html>"#,
        escape_html(title),
        body
    )
}

pub(crate) fn escape_html(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

pub(crate) fn internal_error() -> Response {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Html(page("Error", "<p>Internal server error.</p>")),
    )
        .into_response()
}
