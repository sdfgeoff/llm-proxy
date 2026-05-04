use std::fmt::Write;

use axum::response::{Html, IntoResponse, Response};
use llm_proxy_db::{DashboardMetrics, DashboardPeriod};

use crate::{
    dashboard_charts::{render_dimension_table, render_hourly_charts, render_status_table},
    render::{escape_html, internal_error, page},
    routes::DashboardQuery,
    DashboardState,
};

pub(crate) async fn dashboard_page(state: &DashboardState, query: DashboardQuery) -> Response {
    let period = DashboardPeriod::from_query(query.period.as_deref());
    let Ok(metrics) = state.database.dashboard_metrics(period).await else {
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

    render_period_switcher(&mut body, metrics.period);
    render_overview(&mut body, metrics);
    render_hourly_charts(&mut body, &metrics.hourly);
    render_dimension_table(&mut body, "By model", "Model", &metrics.by_model);
    render_dimension_table(&mut body, "By API key", "API key", &metrics.by_key);
    render_status_table(&mut body, metrics);
    body
}

fn render_period_switcher(body: &mut String, period: DashboardPeriod) {
    body.push_str("<section><h2>Range</h2><p>");
    for candidate in [
        DashboardPeriod::Last24Hours,
        DashboardPeriod::Last7Days,
        DashboardPeriod::Last30Days,
    ] {
        let label = match candidate {
            DashboardPeriod::Last24Hours => "24 hours",
            DashboardPeriod::Last7Days => "7 days",
            DashboardPeriod::Last30Days => "30 days",
        };
        if candidate == period {
            let _ = write!(body, "<strong>{}</strong> ", escape_html(label));
        } else {
            let _ = write!(
                body,
                "<a href=\"/?period={}\">{}</a> ",
                candidate.as_query(),
                escape_html(label)
            );
        }
    }
    body.push_str("</p></section>");
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
