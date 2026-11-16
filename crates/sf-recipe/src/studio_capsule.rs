//! Offline interactive capsule export (`STUDIO-PHASE-4`, section 6.3).
//!
//! The exporter produces one `.safi.html` file that opens from `file://` with the network fully
//! disabled. It contains **no JavaScript at all**: slides and filters are implemented with CSS
//! only, so there is no script execution surface for untrusted text and no CDN, font, or API
//! dependency.
//!
//! CSP uses an exact SHA-256 digest of the single `<style>` block in `<head>` with zero
//! `'unsafe-inline'`, zero `'unsafe-eval'`, and zero inline `style=""` attributes.
//!
//! Every number in the file was computed by Rust at export time and carries its metric name,
//! formula, formula version, dataset version, and `BLAKE3` fingerprint. Any `{{metric:id}}`
//! placeholder in Slides or Narrative Document blocks is resolved strictly via
//! [`crate::studio_dsl::render_narrative_strict`]; unresolved references block export (`SF-CAL-006`).

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;
use sf_core::error::{ErrorCode, ErrorSeverity, SafiError};
use sha2::{Digest as _, Sha256};

use crate::datagraph::{AggFunc, ChartType, ComputedChartPoint, DataGraph, datagraph_display};
use crate::studio_dsl::render_narrative_strict;

/// Maximum accepted size of the generated document (`8 MiB`).
pub const MAX_CAPSULE_BYTES: usize = 8 * 1024 * 1024;

/// Maximum single embedded raster asset size (`1 MiB`).
pub const MAX_ASSET_DATA_URI_BYTES: usize = 1024 * 1024;

/// A chart the caller wants inside the capsule. Aggregation happens in Rust, not in the file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CapsuleChart {
    /// Chart title (already localized by the caller).
    pub title: String,
    /// Visual chart kind (`Bar`, `Line`, `Area`, `Pie`, `Donut`, `Scatter`, `Histogram`, `Kpi`).
    pub chart_type: ChartType,
    /// Aggregation function.
    pub function: AggFunc,
    /// Column being aggregated (or Y column for Scatter).
    pub column: String,
    /// Column used for grouping (or X column for Scatter).
    pub group_by: String,
}

/// A slide from the Quad-Studio Slides editor.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CapsuleSlide {
    /// Slide heading.
    pub title: String,
    /// Slide prose (supports `{{metric:id}}` placeholders).
    pub body: String,
    /// Optional primary metric identifier showcased on the slide.
    pub metric_id: Option<String>,
}

/// A rich block from the Quad-Studio Smart Verified Document editor.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CapsuleDocBlock {
    /// Block kind (`heading`, `paragraph`, `metric_callout`, `callout`).
    pub kind: String,
    /// Block heading or label.
    pub title: String,
    /// Block text (supports `{{metric:id}}` placeholders).
    pub content: String,
    /// Authorship badge (`user` or `ai`).
    pub authored_by: String,
}

/// A positioned block from the Custom Free-Form Builder canvas.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CapsuleFreeFormBlock {
    /// Block kind (`kpi`, `text`, `image`).
    pub kind: String,
    /// Heading shown inside the block.
    pub title: String,
    /// Prose body (supports `{{metric:id}}` placeholders).
    pub body: String,
    /// Optional referenced metric ID.
    pub metric_id: Option<String>,
    /// Optional local raster asset ID.
    pub asset_id: Option<String>,
    /// 1-based grid column (`1..=12`).
    pub col: u8,
    /// 1-based grid row (`1..=32`).
    pub row: u8,
    /// Column span (`1..=12`).
    pub col_span: u8,
    /// Row span (`1..=8`).
    pub row_span: u8,
    /// Visual stacking order (`1..=50`).
    pub z_index: u16,
}

/// An offline raster image embedded locally as a verified `data:image/...` URI.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CapsuleAsset {
    /// Stable asset identifier referenced by free-form or doc blocks.
    pub id: String,
    /// Accessible alt label.
    pub name: String,
    /// Validated `data:image/png|jpeg|webp;base64,...` URI.
    pub data_uri: String,
}

/// Options for [`export_capsule`].
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CapsuleOptions {
    /// Project title shown in the header.
    pub title: String,
    /// ISO-8601 UTC creation timestamp (supplied by the caller, never invented here).
    pub created_at: String,
    /// Dashboard charts.
    pub charts: Vec<CapsuleChart>,
    /// Deck slides.
    pub slides: Vec<CapsuleSlide>,
    /// Narrative report blocks.
    pub doc_blocks: Vec<CapsuleDocBlock>,
    /// Free-form positioned canvas blocks.
    pub freeform_blocks: Vec<CapsuleFreeFormBlock>,
    /// Local raster image assets.
    pub assets: Vec<CapsuleAsset>,
    /// Selected Living Template name, if any.
    pub template_name: Option<String>,
}

fn export_failed() -> SafiError {
    SafiError::new(
        ErrorCode::SF_SYS_001,
        ErrorSeverity::Error,
        "CapsuleExportFailed",
        "studio.capsule_failed",
    )
}

fn invalid_capsule_option() -> SafiError {
    SafiError::new(
        ErrorCode::SF_CAL_002,
        ErrorSeverity::Error,
        "InvalidCapsuleOption",
        "studio.invalid_command",
    )
}

/// Validates that an embedded image URI is strictly base64 PNG/JPEG/WEBP with safe characters.
fn validate_asset_data_uri(uri: &str) -> Result<(), SafiError> {
    if uri.len() > MAX_ASSET_DATA_URI_BYTES {
        return Err(invalid_capsule_option());
    }
    let allowed_prefixes = [
        "data:image/png;base64,",
        "data:image/jpeg;base64,",
        "data:image/webp;base64,",
    ];
    let Some(payload) = allowed_prefixes
        .iter()
        .find_map(|prefix| uri.strip_prefix(prefix))
    else {
        return Err(invalid_capsule_option());
    };
    if payload.is_empty()
        || !payload
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'+' | b'/' | b'='))
    {
        return Err(invalid_capsule_option());
    }
    Ok(())
}

/// Escapes text so it can only ever be data inside HTML.
#[must_use]
pub fn escape_html(text: &str) -> String {
    let mut escaped = String::with_capacity(text.len());
    for character in text.chars() {
        match character {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            '"' => escaped.push_str("&quot;"),
            '\'' => escaped.push_str("&#39;"),
            other => escaped.push(other),
        }
    }
    escaped
}

const STYLE: &str = r#"
:root{color-scheme:light dark}
*{box-sizing:border-box}
body{margin:0;font-family:system-ui,"Segoe UI",Tahoma,"Noto Naskh Arabic",sans-serif;line-height:1.6;background:#0b1220;color:#e8eefc}
main{max-width:1100px;margin:0 auto;padding:32px 20px 64px}
h1{font-size:28px;margin:0 0 8px}
h2{font-size:20px;margin:32px 0 12px}
.badge{display:inline-block;padding:2px 10px;border-radius:999px;background:#1d4ed8;color:#fff;font-size:12px}
.badge-ai{background:#6d28d9}
.meta{color:#9fb0c9;font-size:13px;margin:8px 0 0}
.nav{display:flex;flex-wrap:wrap;gap:10px;margin:16px 0}
.nav a{color:#93c5fd;text-decoration:none;border:1px solid #2b3b58;border-radius:999px;padding:4px 12px;font-size:13px}
.grid{display:grid;grid-template-columns:repeat(auto-fit,minmax(260px,1fr));gap:16px}
.ff-grid{display:grid;grid-template-columns:repeat(12,minmax(0,1fr));grid-auto-rows:minmax(72px,auto);gap:12px}
.card{background:#121b2d;border:1px solid #1f2b42;border-radius:14px;padding:16px}
.card h3{margin:0 0 8px;font-size:16px}
.doc-stack{display:flex;flex-direction:column;gap:12px}
.callout{border-inline-start:4px solid #38bdf8}
.value{font-size:26px;font-weight:700;margin:4px 0}
.provenance{color:#8fa3c0;font-size:12px;word-break:break-all}
table{width:100%;border-collapse:collapse;font-size:13px}
th,td{text-align:start;padding:8px;border-bottom:1px solid #1f2b42;vertical-align:top}
.bars{display:grid;gap:8px}
.bar{display:flex;align-items:center;gap:10px}
.bar span{min-width:90px}
.track{flex:1;background:#1b2740;border-radius:999px;height:12px;overflow:hidden}
.fill{height:100%;background:linear-gradient(90deg,#2563eb,#22d3ee)}
.chart-svg{width:100%;height:140px;display:block;margin:8px 0;background:#0e1726;border-radius:10px}
.chart-svg-circle{max-width:180px;height:160px;margin:8px auto}
.legend{display:grid;gap:4px;margin:8px 0;padding:0;list-style:none;font-size:12px}
.legend li{display:flex;justify-content:space-between;gap:8px;padding:3px 6px;border-radius:6px;background:#0e1726}
.filters{display:flex;flex-wrap:wrap;gap:8px;margin:8px 0 12px}
.filters input{position:absolute;opacity:0;pointer-events:none}
.filters label{cursor:pointer;padding:4px 10px;border-radius:999px;border:1px solid #2b3b58;font-size:12px}
.asset-img{max-width:100%;height:auto;border-radius:10px;display:block}
@media print{.filters,.nav{display:none}}
"#;

fn width_utility_rules() -> String {
    let mut rules = String::new();
    for pct in 0_u32..=100 {
        rules.push_str(&format!(".w{pct}{{width:{pct}%}}"));
    }
    rules
}

fn filter_rules(chart_index: usize, points: &[ComputedChartPoint]) -> String {
    let mut rules = String::new();
    for (group_index, _) in points.iter().enumerate() {
        rules.push_str(&format!(
            "#c{chart_index}-{group_index}:checked ~ .bars .bar{{display:none}}"
        ));
        rules.push_str(&format!(
            "#c{chart_index}-{group_index}:checked ~ .bars .bar.g{group_index}{{display:flex}}"
        ));
    }
    rules
}

const SLICE_PALETTE: [&str; 6] = [
    "#2563eb", "#0891b2", "#7c3aed", "#059669", "#d97706", "#db2777",
];

fn render_chart_body(
    body: &mut String,
    dynamic_css: &mut String,
    chart_index: usize,
    chart: &CapsuleChart,
    series: &crate::datagraph::ComputedChartSeries,
) {
    match chart.chart_type {
        ChartType::Kpi => {
            body.push_str(&format!(
                "<p class=\"value\" data-kpi-value>{}</p>\n",
                escape_html(&series.total_or_kpi)
            ));
        }
        ChartType::Pie | ChartType::Donut => {
            let is_donut = matches!(chart.chart_type, ChartType::Donut);
            let slice_attr = if is_donut {
                "data-donut-slice"
            } else {
                "data-pie-slice"
            };
            body.push_str(
                "<svg class=\"chart-svg chart-svg-circle\" viewBox=\"0 0 100 100\" role=\"img\">\n",
            );
            for (idx, point) in series.points.iter().enumerate() {
                if let Some(d) = &point.svg_path {
                    let color = SLICE_PALETTE[idx % SLICE_PALETTE.len()];
                    body.push_str(&format!(
                        "<path {slice_attr}=\"{idx}\" d=\"{}\" fill=\"{color}\"></path>\n",
                        escape_html(d)
                    ));
                }
            }
            if is_donut {
                body.push_str(&format!(
                    "<text x=\"50\" y=\"53\" text-anchor=\"middle\" fill=\"#e8eefc\" font-size=\"9\" font-weight=\"700\">{}</text>\n",
                    escape_html(&series.total_or_kpi)
                ));
            }
            body.push_str("</svg>\n<ul class=\"legend\">\n");
            for point in &series.points {
                let share = point.share_pct.as_deref().unwrap_or("0");
                body.push_str(&format!(
                    "<li><span>{}</span><span class=\"provenance\">{} ({}%)</span></li>\n",
                    escape_html(&point.label),
                    escape_html(&point.value),
                    escape_html(share)
                ));
            }
            body.push_str("</ul>\n");
        }
        ChartType::Line | ChartType::Area | ChartType::Scatter => {
            body.push_str("<svg class=\"chart-svg\" viewBox=\"0 0 100 100\" preserveAspectRatio=\"none\" role=\"img\">\n");
            if matches!(chart.chart_type, ChartType::Scatter) {
                body.push_str("<line x1=\"8\" y1=\"92\" x2=\"94\" y2=\"92\" stroke=\"#334155\" stroke-width=\"1\"></line>\n");
                body.push_str("<line x1=\"8\" y1=\"8\" x2=\"8\" y2=\"92\" stroke=\"#334155\" stroke-width=\"1\"></line>\n");
            }
            if matches!(chart.chart_type, ChartType::Line | ChartType::Area)
                && !series.points.is_empty()
            {
                let poly = series
                    .points
                    .iter()
                    .map(|p| format!("{},{}", p.x_pct, p.y_pct))
                    .collect::<Vec<_>>()
                    .join(" ");
                if matches!(chart.chart_type, ChartType::Area)
                    && let (Some(first), Some(last)) = (series.points.first(), series.points.last())
                {
                    body.push_str(&format!(
                        "<polygon points=\"{},90 {} {},90\" fill=\"#1d4ed8\" fill-opacity=\"0.35\"></polygon>\n",
                        first.x_pct, poly, last.x_pct
                    ));
                }
                body.push_str(&format!(
                    "<polyline points=\"{poly}\" fill=\"none\" stroke=\"#38bdf8\" stroke-width=\"2.5\"></polyline>\n"
                ));
            }
            for point in &series.points {
                body.push_str(&format!(
                    "<circle cx=\"{}\" cy=\"{}\" r=\"2.5\" fill=\"#22d3ee\"></circle>\n",
                    point.x_pct, point.y_pct
                ));
            }
            body.push_str("</svg>\n<ul class=\"legend\">\n");
            for point in &series.points {
                let detail = match (&point.x_value, &point.y_value) {
                    (Some(x), Some(y)) => format!("({x}, {y})"),
                    _ => point.value.clone(),
                };
                body.push_str(&format!(
                    "<li><span>{}</span><span class=\"provenance\">{}</span></li>\n",
                    escape_html(&point.label),
                    escape_html(&detail),
                ));
            }
            body.push_str("</ul>\n");
        }
        ChartType::Histogram => {
            body.push_str("<svg class=\"chart-svg\" viewBox=\"0 0 100 100\" preserveAspectRatio=\"none\" role=\"img\">\n");
            let count = series.points.len().max(1);
            let slot_width = 84.0 / (count as f64);
            let bar_width = (slot_width - 2.0).max(4.0);
            for (idx, point) in series.points.iter().enumerate() {
                let x = 8.0 + (idx as f64) * slot_width;
                let h = ((f64::from(point.ratio_pct.min(100)) * 78.0) / 100.0).max(2.0);
                let y = 90.0 - h;
                body.push_str(&format!(
                    "<rect data-histogram-bin=\"{idx}\" x=\"{x:.2}\" y=\"{y:.2}\" width=\"{bar_width:.2}\" height=\"{h:.2}\" fill=\"#38bdf8\"></rect>\n"
                ));
            }
            body.push_str("</svg>\n<ul class=\"legend\">\n");
            for point in &series.points {
                body.push_str(&format!(
                    "<li><span>{}</span><span class=\"provenance\">{}</span></li>\n",
                    escape_html(&point.label),
                    escape_html(&point.value)
                ));
            }
            body.push_str("</ul>\n");
        }
        ChartType::Bar => {
            if series.points.len() > 1 {
                body.push_str("<div class=\"filters\">\n");
                body.push_str(&format!("<label for=\"c{chart_index}-all\">All</label>\n"));
                for (group_index, point) in series.points.iter().enumerate() {
                    body.push_str(&format!(
                        "<label for=\"c{chart_index}-{group_index}\">{}</label>\n",
                        escape_html(&point.label)
                    ));
                }
                body.push_str("</div>\n");
                body.push_str(&format!(
                    "<input type=\"radio\" name=\"f{chart_index}\" id=\"c{chart_index}-all\" checked>\n"
                ));
                for (group_index, _) in series.points.iter().enumerate() {
                    body.push_str(&format!(
                        "<input type=\"radio\" name=\"f{chart_index}\" id=\"c{chart_index}-{group_index}\">\n"
                    ));
                }
                dynamic_css.push_str(&filter_rules(chart_index, &series.points));
            }
            body.push_str("<div class=\"bars\">\n");
            for (group_index, point) in series.points.iter().enumerate() {
                let pct = point.ratio_pct.min(100);
                body.push_str(&format!(
                    "<div class=\"bar g{group_index}\"><span>{}</span><div class=\"track\"><div class=\"fill w{pct}\"></div></div><span class=\"provenance\">{}</span></div>\n",
                    escape_html(&point.label),
                    escape_html(&point.value),
                ));
            }
            body.push_str("</div>\n");
        }
    }
}

/// Renders the self-contained capsule document without `unsafe-inline` or JavaScript.
///
/// # Errors
/// `SF-CAL-006` when any `{{metric:...}}` reference in slides, doc blocks, or free-form blocks is
/// missing, `SF-CAL-002` when a raster asset URI is unsafe, or `SF-SYS-001` when the output
/// exceeds [`MAX_CAPSULE_BYTES`].
#[allow(clippy::too_many_lines)]
pub fn export_capsule(graph: &DataGraph, options: &CapsuleOptions) -> Result<String, SafiError> {
    let mut assets_by_id = std::collections::BTreeMap::new();
    for asset in &options.assets {
        validate_asset_data_uri(&asset.data_uri)?;
        let _ = assets_by_id.insert(asset.id.as_str(), asset);
    }

    let mut body = String::with_capacity(16 * 1024);
    let mut dynamic_css = width_utility_rules();

    body.push_str(&format!(
        "<h1>{}</h1>\n<p><span class=\"badge\">snapshot</span>",
        escape_html(&options.title)
    ));
    if let Some(tpl) = &options.template_name {
        body.push_str(&format!(
            " <span class=\"badge\">template: {}</span>",
            escape_html(tpl)
        ));
    }
    body.push_str("</p>\n");
    body.push_str(&format!(
        "<p class=\"meta\">Created: {} · Dataset version: {} · Revision: {} · Source: {}</p>\n",
        escape_html(&options.created_at),
        graph.dataset_version().get(),
        graph.revision().get(),
        escape_html(graph.source_id().as_str()),
    ));
    body.push_str(
        "<p class=\"meta\">This file is a fixed snapshot computed by Safi. It is not a live connection: edits made here never return to the project, and every number below is bound to the fingerprint stored next to it.</p>\n",
    );
    body.push_str(
        "<nav class=\"nav\" aria-label=\"Capsule sections\"><a href=\"#dashboard\">Dashboard</a><a href=\"#slides\">Slides</a><a href=\"#report\">Report</a><a href=\"#metrics\">Metrics Provenance</a></nav>\n",
    );

    // Dashboard charts
    body.push_str("<h2 id=\"dashboard\">Dashboard</h2>\n<div class=\"grid\">\n");
    for (chart_index, chart) in options.charts.iter().enumerate() {
        let series = graph.compute_chart_series(
            chart.chart_type,
            chart.function,
            &chart.column,
            &chart.group_by,
        )?;
        body.push_str(&format!(
            "<section class=\"card chart\" data-chart-type=\"{}\">\n",
            escape_html(chart.chart_type.as_str())
        ));
        body.push_str(&format!(
            "<h3>{} <span class=\"badge\">{}</span></h3>\n",
            escape_html(&chart.title),
            escape_html(chart.chart_type.as_str())
        ));
        render_chart_body(&mut body, &mut dynamic_css, chart_index, chart, &series);
        body.push_str(&format!(
            "<p class=\"provenance\">{} · {} of {} grouped by {}</p>\n",
            escape_html(chart.chart_type.as_str()),
            escape_html(chart.function.as_str()),
            escape_html(&chart.column),
            escape_html(&chart.group_by)
        ));
        body.push_str("</section>\n");
    }
    body.push_str("</div>\n");

    // Slides (authored slides if supplied, or metric deck fallback)
    body.push_str("<h2 id=\"slides\">Slides</h2>\n<div class=\"grid\">\n");
    if options.slides.is_empty() {
        for id in graph.metric_ids() {
            let Some(token) = graph.token(&id) else {
                continue;
            };
            body.push_str("<section class=\"card\">\n");
            body.push_str(&format!("<h3>{}</h3>\n", escape_html(&token.metric_name)));
            body.push_str(&format!(
                "<p class=\"value\">{} {}</p>\n",
                escape_html(&datagraph_display(&token.value)),
                escape_html(token.unit.as_deref().unwrap_or(""))
            ));
            body.push_str(&format!(
                "<p class=\"provenance\">formula: {} · v{} · data v{} · {}…</p>\n",
                escape_html(&token.formula),
                token.formula_version,
                token.source_version.get(),
                escape_html(&token.fingerprint()[..token.fingerprint().len().min(16)]),
            ));
            body.push_str("</section>\n");
        }
    } else {
        for slide in &options.slides {
            let rendered_body = render_narrative_strict(&slide.body, graph)?;
            body.push_str("<section class=\"card\">\n");
            body.push_str(&format!("<h3>{}</h3>\n", escape_html(&slide.title)));
            if let Some(mid) = &slide.metric_id {
                let metric_id = sf_core::datagraph::MetricId::new(mid)?;
                if let Some(token) = graph.token(&metric_id) {
                    body.push_str(&format!(
                        "<p class=\"value\">{} {}</p>\n",
                        escape_html(&datagraph_display(&token.value)),
                        escape_html(token.unit.as_deref().unwrap_or(""))
                    ));
                }
            }
            body.push_str(&format!("<p>{}</p>\n", escape_html(&rendered_body.text)));
            if !rendered_body.references.is_empty() {
                let refs = rendered_body
                    .references
                    .iter()
                    .map(|r| {
                        format!(
                            "{}:{}",
                            escape_html(r.metric_id.as_str()),
                            escape_html(&r.fingerprint[..r.fingerprint.len().min(12)])
                        )
                    })
                    .collect::<Vec<_>>()
                    .join(" · ");
                body.push_str(&format!(
                    "<p class=\"provenance\">Verified refs: {refs}</p>\n"
                ));
            }
            body.push_str("</section>\n");
        }
    }
    body.push_str("</div>\n");

    // Narrative document blocks
    if !options.doc_blocks.is_empty() {
        body.push_str("<h2 id=\"report\">Executive Report</h2>\n<div class=\"doc-stack\">\n");
        for block in &options.doc_blocks {
            let rendered = render_narrative_strict(&block.content, graph)?;
            let callout_class = if block.kind == "callout" || block.kind == "metric_callout" {
                "card callout"
            } else {
                "card"
            };
            body.push_str(&format!("<article class=\"{callout_class}\">\n"));
            if !block.title.trim().is_empty() {
                body.push_str(&format!("<h3>{}</h3>\n", escape_html(&block.title)));
            }
            if block.authored_by == "ai" {
                body.push_str("<p><span class=\"badge badge-ai\">AI-assisted draft</span></p>\n");
            }
            body.push_str(&format!("<p>{}</p>\n", escape_html(&rendered.text)));
            body.push_str("</article>\n");
        }
        body.push_str("</div>\n");
    }

    // Free-form canvas blocks (CSS rules placed inside hashed head <style>)
    if !options.freeform_blocks.is_empty() {
        body.push_str("<h2 id=\"canvas\">Free-Form Canvas</h2>\n<div class=\"ff-grid\">\n");
        for (idx, block) in options.freeform_blocks.iter().enumerate() {
            let rendered = render_narrative_strict(&block.body, graph)?;
            let col_start = block.col.clamp(1, 12);
            let col_span = block.col_span.clamp(1, 12);
            let row_start = block.row.clamp(1, 32);
            let row_span = block.row_span.clamp(1, 8);
            let z = block.z_index.clamp(1, 50);
            dynamic_css.push_str(&format!(
                "#ff-{idx}{{grid-column:{col_start} / span {col_span};grid-row:{row_start} / span {row_span};z-index:{z}}}"
            ));
            body.push_str(&format!(
                "<section id=\"ff-{idx}\" class=\"card\" data-grid-col=\"{col_start}\" data-grid-row=\"{row_start}\" data-grid-col-span=\"{col_span}\" data-grid-row-span=\"{row_span}\">\n"
            ));
            if !block.title.trim().is_empty() {
                body.push_str(&format!("<h3>{}</h3>\n", escape_html(&block.title)));
            }
            if let Some(mid) = &block.metric_id {
                let metric_id = sf_core::datagraph::MetricId::new(mid)?;
                if let Some(token) = graph.token(&metric_id) {
                    body.push_str(&format!(
                        "<p class=\"value\">{} {}</p>\n",
                        escape_html(&datagraph_display(&token.value)),
                        escape_html(token.unit.as_deref().unwrap_or(""))
                    ));
                }
            }
            if let Some(asset_id) = &block.asset_id
                && let Some(asset) = assets_by_id.get(asset_id.as_str())
            {
                body.push_str(&format!(
                    "<img class=\"asset-img\" src=\"{}\" alt=\"{}\">\n",
                    asset.data_uri,
                    escape_html(&asset.name)
                ));
            }
            if !rendered.text.trim().is_empty() {
                body.push_str(&format!("<p>{}</p>\n", escape_html(&rendered.text)));
            }
            body.push_str("</section>\n");
        }
        body.push_str("</div>\n");
    }

    // Verified metrics provenance table
    body.push_str("<h2 id=\"metrics\">Verified metrics</h2>\n<table>\n<thead><tr><th>Metric</th><th>Value</th><th>Formula</th><th>Version</th><th>Fingerprint</th></tr></thead>\n<tbody>\n");
    for id in graph.metric_ids() {
        let Some(token) = graph.token(&id) else {
            continue;
        };
        body.push_str(&format!(
            "<tr><td>{}</td><td>{} {}</td><td>{}</td><td>v{} / data v{}</td><td class=\"provenance\">{}</td></tr>\n",
            escape_html(&token.metric_name),
            escape_html(&datagraph_display(&token.value)),
            escape_html(token.unit.as_deref().unwrap_or("")),
            escape_html(&token.formula),
            token.formula_version,
            token.source_version.get(),
            escape_html(token.fingerprint()),
        ));
    }
    body.push_str("</tbody>\n</table>\n");
    body.push_str("<p class=\"meta\">Filter controls only select between aggregates computed by Safi before export; they never calculate a new number.</p>\n");

    let stylesheet = format!("{STYLE}{dynamic_css}");
    let digest = Sha256::digest(stylesheet.as_bytes());
    let style_hash = BASE64_STANDARD.encode(digest);

    let mut html = String::with_capacity(stylesheet.len() + body.len() + 512);
    html.push_str(
        "<!DOCTYPE html>\n<html lang=\"en\" dir=\"auto\">\n<head>\n<meta charset=\"utf-8\">\n",
    );
    html.push_str("<meta name=\"viewport\" content=\"width=device-width,initial-scale=1\">\n");
    html.push_str(&format!(
        "<meta http-equiv=\"Content-Security-Policy\" content=\"default-src 'none'; script-src 'none'; style-src 'sha256-{style_hash}'; img-src data:; base-uri 'none'; form-action 'none'\">\n"
    ));
    html.push_str(&format!(
        "<title>{}</title>\n<style>{stylesheet}</style>\n</head>\n<body>\n<main>\n{body}</main>\n</body>\n</html>\n",
        escape_html(&options.title)
    ));

    if html.len() > MAX_CAPSULE_BYTES {
        return Err(export_failed());
    }
    Ok(html)
}

#[cfg(test)]
mod tests {
    use sf_core::column::{Column, ColumnSchema, ColumnType, ConfidenceScore};
    use sf_core::datagraph::{MetricId, SourceId};
    use sf_core::error::{ErrorCode, SafiError};
    use sf_core::table::Table;
    use sf_core::value::CellValue;

    use super::{
        CapsuleAsset, CapsuleChart, CapsuleDocBlock, CapsuleFreeFormBlock, CapsuleOptions,
        CapsuleSlide, escape_html, export_capsule,
    };
    use crate::datagraph::{AggFunc, ChartType, DataGraph};

    fn graph() -> Result<DataGraph, SafiError> {
        let columns = [
            ("region", ["Cairo", "Giza", "Cairo"].as_slice()),
            ("amount", ["100", "200", "50"].as_slice()),
        ]
        .into_iter()
        .enumerate()
        .map(|(index, (name, values))| {
            Column::new(
                ColumnSchema::new(index, name, ColumnType::Unknown, ConfidenceScore::ONE),
                values
                    .iter()
                    .map(|value| CellValue::from_raw(*value))
                    .collect(),
            )
        })
        .collect();
        let table = Table::new("tbl_sales", "sales", columns)?;
        let mut graph = DataGraph::new(SourceId::new("sales")?, table, 10)?;
        graph.define_metric(
            MetricId::new("total")?,
            "Total",
            Some("EGP"),
            "SUM(amount)",
            10,
        )?;
        Ok(graph)
    }

    fn options(title: &str) -> CapsuleOptions {
        CapsuleOptions {
            title: title.to_owned(),
            created_at: "2026-11-11T00:00:00Z".to_owned(),
            charts: vec![CapsuleChart {
                title: "Revenue by region".to_owned(),
                chart_type: ChartType::Bar,
                function: AggFunc::Sum,
                column: "amount".to_owned(),
                group_by: "region".to_owned(),
            }],
            ..CapsuleOptions::default()
        }
    }

    #[test]
    fn the_capsule_carries_verified_values_and_their_provenance() -> Result<(), SafiError> {
        let graph = graph()?;
        let html = export_capsule(&graph, &options("Board review"))?;
        let token = graph.token(&MetricId::new("total")?).ok_or_else(|| {
            SafiError::new(
                ErrorCode::SF_CAL_006,
                sf_core::error::ErrorSeverity::Error,
                "UnknownMetric",
                "studio.unknown_metric",
            )
        })?;
        assert!(html.contains("350"));
        assert!(html.contains(token.fingerprint()));
        assert!(html.contains("Dataset version: 0"));
        assert!(html.contains("SUM(amount)"));
        assert!(html.contains("snapshot"));
        assert!(html.contains("Cairo"));
        assert!(!html.contains("unsafe-inline"));
        assert!(!html.contains("style=\""));
        assert!(html.contains("style-src 'sha256-"));
        Ok(())
    }

    #[test]
    fn the_capsule_contains_no_script_surface_or_external_resource() -> Result<(), SafiError> {
        let graph = graph()?;
        let html = export_capsule(&graph, &options("<script>alert(1)</script>"))?;
        assert!(!html.contains("<script"));
        assert!(!html.to_lowercase().contains("javascript:"));
        assert!(!html.contains("http://"));
        assert!(!html.contains("https://"));
        assert!(html.contains("&lt;script&gt;"));
        assert!(html.contains("script-src 'none'"));
        Ok(())
    }

    #[test]
    fn hostile_text_is_escaped_in_every_position() -> Result<(), SafiError> {
        assert_eq!(
            escape_html("<img src=x onerror=alert(1)>"),
            "&lt;img src=x onerror=alert(1)&gt;"
        );
        let graph = graph()?;
        let mut opts = options("ok");
        opts.charts[0].title = "<b>bold</b>".to_owned();
        let html = export_capsule(&graph, &opts)?;
        assert!(html.contains("&lt;b&gt;bold&lt;/b&gt;"));
        assert!(!html.contains("<b>bold</b>"));
        Ok(())
    }

    #[test]
    fn full_slides_docs_freeform_assets_and_strict_metric_gate_work() -> Result<(), SafiError> {
        let graph = graph()?;
        let mut opts = options("Full Quad Studio");
        opts.slides = vec![CapsuleSlide {
            title: "Executive Summary".to_owned(),
            body: "Revenue reached {{metric:total}} EGP.".to_owned(),
            metric_id: Some("total".to_owned()),
        }];
        opts.doc_blocks = vec![CapsuleDocBlock {
            kind: "callout".to_owned(),
            title: "Decision".to_owned(),
            content: "Baseline locked at {{metric:total}}.".to_owned(),
            authored_by: "user".to_owned(),
        }];
        opts.assets = vec![CapsuleAsset {
            id: "logo1".to_owned(),
            name: "Logo".to_owned(),
            data_uri: "data:image/png;base64,iVBORw0KGgo=".to_owned(),
        }];
        opts.freeform_blocks = vec![CapsuleFreeFormBlock {
            kind: "image".to_owned(),
            title: "Banner".to_owned(),
            body: "Total {{metric:total}}".to_owned(),
            metric_id: Some("total".to_owned()),
            asset_id: Some("logo1".to_owned()),
            col: 1,
            row: 1,
            col_span: 6,
            row_span: 2,
            z_index: 2,
        }];
        let html = export_capsule(&graph, &opts)?;
        assert!(html.contains("Revenue reached 350 EGP."));
        assert!(html.contains("Baseline locked at 350."));
        assert!(html.contains("id=\"ff-0\""));
        assert!(html.contains("data:image/png;base64,iVBORw0KGgo="));

        // Unresolved metric ref in slide or doc must fail with SF-CAL-006
        let mut missing_ref = opts.clone();
        missing_ref.doc_blocks[0].content = "Broken {{metric:nonexistent}}".to_owned();
        assert_eq!(
            export_capsule(&graph, &missing_ref).err().map(|e| e.code()),
            Some(ErrorCode::SF_CAL_006)
        );

        // Unsafe SVG asset URI must fail with SF-CAL-002
        let mut bad_asset = opts;
        bad_asset.assets[0].data_uri = "data:image/svg+xml;base64,PHN2Zz4=".to_owned();
        assert_eq!(
            export_capsule(&graph, &bad_asset).err().map(|e| e.code()),
            Some(ErrorCode::SF_CAL_002)
        );
        Ok(())
    }

    #[test]
    fn capsule_renders_genuine_svg_geometry_for_pie_donut_histogram_and_standalone_kpi()
    -> Result<(), SafiError> {
        let mut g = graph()?;
        let qty = Column::new(
            ColumnSchema::new(2, "qty", ColumnType::Unknown, ConfidenceScore::ONE),
            ["2", "4", "1"]
                .into_iter()
                .map(CellValue::from_raw)
                .collect(),
        );
        let mut cols = g.working().columns().to_vec();
        cols.push(qty);
        let table = Table::new("tbl_sales", "sales", cols)?;
        g = DataGraph::new(SourceId::new("sales")?, table, 10)?;

        let opts = CapsuleOptions {
            title: "All 8 Visuals".to_owned(),
            created_at: "2026-11-16T00:00:00Z".to_owned(),
            charts: vec![
                CapsuleChart {
                    title: "Bar".to_owned(),
                    chart_type: ChartType::Bar,
                    function: AggFunc::Sum,
                    column: "amount".to_owned(),
                    group_by: "region".to_owned(),
                },
                CapsuleChart {
                    title: "Pie".to_owned(),
                    chart_type: ChartType::Pie,
                    function: AggFunc::Sum,
                    column: "amount".to_owned(),
                    group_by: "region".to_owned(),
                },
                CapsuleChart {
                    title: "Donut".to_owned(),
                    chart_type: ChartType::Donut,
                    function: AggFunc::Sum,
                    column: "amount".to_owned(),
                    group_by: "region".to_owned(),
                },
                CapsuleChart {
                    title: "Histogram".to_owned(),
                    chart_type: ChartType::Histogram,
                    function: AggFunc::Count,
                    column: "amount".to_owned(),
                    group_by: "region".to_owned(),
                },
                CapsuleChart {
                    title: "KPI".to_owned(),
                    chart_type: ChartType::Kpi,
                    function: AggFunc::Sum,
                    column: "amount".to_owned(),
                    group_by: "region".to_owned(),
                },
            ],
            ..CapsuleOptions::default()
        };
        let html = export_capsule(&g, &opts)?;
        assert!(html.contains("data-pie-slice=\"0\""));
        assert!(html.contains("data-donut-slice=\"0\""));
        assert!(html.contains("data-histogram-bin=\"0\""));
        assert!(html.contains("<rect data-histogram-bin="));
        assert!(html.contains("<p class=\"value\" data-kpi-value>350</p>"));
        // KPI card section must not render .bars rows
        let kpi_section = html
            .split("data-chart-type=\"kpi\"")
            .nth(1)
            .and_then(|s| s.split("</section>").next())
            .unwrap_or("");
        assert!(!kpi_section.contains("class=\"bars\""));
        assert!(!kpi_section.contains("class=\"track\""));
        Ok(())
    }
}
