use std::fmt::Write;

use axum::response::{Html, IntoResponse, Response};
use llm_proxy_db::{DashboardMetrics, DimensionMetric, HourlyMetric};

use crate::{
    render::{escape_html, internal_error, page},
    DashboardState,
};

pub(crate) async fn dashboard_page(state: &DashboardState) -> Response {
    let Ok(metrics) = state.database.dashboard_metrics().await else {
        return internal_error();
    };

    Html(page("LLM Proxy", &render_dashboard_body(state, &metrics))).into_response()
}

fn render_dashboard_body(state: &DashboardState, metrics: &DashboardMetrics) -> String {
    let mut body = String::new();
    let _ = write!(
        body,
        "<h1>LLM Proxy</h1>\
         <nav><a href=\"/requests\">Requests</a> \
         <a href=\"/keys\">Proxy API keys</a> \
         <a href=\"/upstream-secrets\">Upstream secrets</a> \
         <form method=\"post\" action=\"/logout\"><button type=\"submit\">Log out</button></form></nav>\
         <section><h2>Runtime</h2><dl>\
         <dt>Proxy listen</dt><dd>{}</dd>\
         <dt>Admin listen</dt><dd>{}</dd>\
         <dt>Default route</dt><dd>{}</dd>\
         </dl></section>",
        state.config.proxy_listen, state.config.admin_listen, state.config.default_route
    );

    render_overview(&mut body, metrics);
    render_hourly_charts(&mut body, &metrics.hourly);
    render_dimension_table(&mut body, "By model", "Model", &metrics.by_model);
    render_dimension_table(&mut body, "By API key", "API key", &metrics.by_key);
    render_status_table(&mut body, metrics);
    body
}

fn render_overview(body: &mut String, metrics: &DashboardMetrics) {
    let overview = &metrics.overview;
    let _ = write!(
        body,
        "<section><h2>Overview</h2><dl>\
         <dt>Requests</dt><dd>{}</dd>\
         <dt>Total tokens</dt><dd>{}</dd>\
         <dt>Input tokens</dt><dd>{}</dd>\
         <dt>Output tokens</dt><dd>{}</dd>\
         <dt>Average duration</dt><dd>{}</dd>\
         <dt>Average tokens/sec</dt><dd>{}</dd>\
         <dt>Average TTFT</dt><dd>{}</dd>\
         <dt>Errors</dt><dd>{}</dd>\
         </dl></section>",
        overview.request_count,
        overview.total_tokens,
        overview.input_tokens,
        overview.output_tokens,
        format_float_ms(overview.avg_duration_ms),
        format_float(overview.avg_tokens_per_second),
        format_float_ms(overview.avg_time_to_first_token_ms),
        overview.error_count
    );
}

fn render_hourly_charts(body: &mut String, hourly: &[HourlyMetric]) {
    body.push_str("<section><h2>Last 24 hours</h2>");
    if hourly.is_empty() {
        body.push_str("<p>No requests recorded in the last 24 hours.</p></section>");
        return;
    }
    body.push_str("<h3>Tokens</h3>");
    render_line_chart(
        body,
        hourly.iter().map(|point| point.total_tokens as f64),
        |idx| hourly[idx].bucket.clone(),
    );
    body.push_str("<h3>Tokens/sec</h3>");
    render_line_chart(
        body,
        hourly
            .iter()
            .map(|point| point.avg_tokens_per_second.unwrap_or(0.0)),
        |idx| hourly[idx].bucket.clone(),
    );
    body.push_str("<h3>Time to first token</h3>");
    render_line_chart(
        body,
        hourly
            .iter()
            .map(|point| point.avg_time_to_first_token_ms.unwrap_or(0.0)),
        |idx| hourly[idx].bucket.clone(),
    );
    body.push_str("</section>");
}

fn render_dimension_table(
    body: &mut String,
    title: &str,
    label_heading: &str,
    rows: &[DimensionMetric],
) {
    let max_tokens = rows
        .iter()
        .map(|row| row.total_tokens)
        .max()
        .unwrap_or_default()
        .max(1);
    let _ = write!(
        body,
        "<section><h2>{}</h2><table><thead><tr>\
         <th>{}</th><th>Requests</th><th>Tokens</th><th>Tokens/sec</th><th>TTFT</th><th></th>\
         </tr></thead><tbody>",
        escape_html(title),
        escape_html(label_heading)
    );
    for row in rows {
        let width = (row.total_tokens * 100 / max_tokens).max(1);
        let _ = write!(
            body,
            "<tr><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td>\
             <td><div style=\"background:#2563eb;height:0.75rem;width:{}%;\"></div></td></tr>",
            escape_html(&row.label),
            row.request_count,
            row.total_tokens,
            format_float(row.avg_tokens_per_second),
            format_float_ms(row.avg_time_to_first_token_ms),
            width
        );
    }
    body.push_str("</tbody></table></section>");
}

fn render_status_table(body: &mut String, metrics: &DashboardMetrics) {
    body.push_str(
        "<section><h2>Status rates</h2><table><thead><tr><th>Status</th><th>Requests</th></tr></thead><tbody>",
    );
    for row in &metrics.by_status {
        let _ = write!(
            body,
            "<tr><td>{}</td><td>{}</td></tr>",
            escape_html(&row.label),
            row.request_count
        );
    }
    body.push_str("</tbody></table></section>");
}

fn render_line_chart<I, F>(body: &mut String, values: I, label: F)
where
    I: IntoIterator<Item = f64>,
    F: Fn(usize) -> String,
{
    let values: Vec<f64> = values.into_iter().collect();
    let max = values.iter().copied().fold(0.0_f64, f64::max).max(1.0);
    let width = 640.0;
    let height = 160.0;
    let step = if values.len() > 1 {
        width / (values.len() - 1) as f64
    } else {
        width
    };
    let points = values
        .iter()
        .enumerate()
        .map(|(idx, value)| {
            let x = idx as f64 * step;
            let y = height - (value / max * height);
            format!("{x:.1},{y:.1}")
        })
        .collect::<Vec<_>>()
        .join(" ");
    let first = label(0);
    let last = label(values.len().saturating_sub(1));
    let _ = write!(
        body,
        "<svg viewBox=\"0 0 700 210\" role=\"img\" style=\"width:100%;max-width:720px;height:auto;\">\
         <polyline fill=\"none\" stroke=\"#2563eb\" stroke-width=\"3\" points=\"{}\" transform=\"translate(30 20)\"/>\
         <text x=\"30\" y=\"200\">{}</text><text x=\"560\" y=\"200\">{}</text>\
         <text x=\"30\" y=\"18\">max {}</text></svg>",
        points,
        escape_html(&first),
        escape_html(&last),
        format_float(Some(max))
    );
}

fn format_float(value: Option<f64>) -> String {
    value
        .map(|value| format!("{value:.2}"))
        .unwrap_or_else(|| "-".to_owned())
}

fn format_float_ms(value: Option<f64>) -> String {
    value
        .map(|value| format!("{value:.0} ms"))
        .unwrap_or_else(|| "-".to_owned())
}
