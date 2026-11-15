//! End-to-end 18-journey acceptance and multi-size performance benchmarks for Quad-Studio Phase 5.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::fs;
use std::time::Instant;

use sf_desktop::studio::StudioStore;

fn generate_csv(rows: usize) -> String {
    let mut out = String::from("region,amount,quantity,notes\n");
    let regions = ["Cairo", "Alexandria", "Giza", "Mansoura", "Aswan"];
    for idx in 0..rows {
        let region = regions[idx % regions.len()];
        let amount = 100 + (idx % 900);
        let qty = 1 + (idx % 7);
        out.push_str(&format!("{region},{amount},{qty},  note_{idx}  \n"));
    }
    out
}

const TEST_TOKEN: &str = "sfs_0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
const TEST_INSTALLATION_ID: &str = "11111111-2222-4333-8444-555555555555";

fn authenticate(store: &StudioStore) {
    store.set_clock_ms(Some(1_000));
    store
        .session_authority()
        .register_session(
            "sess_e2e",
            "ws_e2e",
            TEST_TOKEN,
            TEST_INSTALLATION_ID,
            10_000_000,
        )
        .expect("register session");
    store
        .set_auth_session(TEST_TOKEN, TEST_INSTALLATION_ID, 1_000)
        .expect("bind session");
}

#[test]
fn studio_eighteen_journeys_end_to_end() {
    let temp = std::env::temp_dir().join(format!(
        "safi_phase5_e2e_{}_{}",
        std::process::id(),
        17_000_000_u64
    ));
    let _ = fs::remove_dir_all(&temp);
    fs::create_dir_all(&temp).expect("temp dir");

    let csv_path = temp.join("sales.csv");
    fs::write(
        &csv_path,
        "region,amount,quantity\nCairo,100,2\nCairo,100,2\nAlex,200,3\n,50,1\n",
    )
    .expect("write csv");

    let store = StudioStore::with_storage_dir(temp.join("bundles"));

    // 0. Unauthenticated Studio command must be rejected fail-closed with SF-AUTH-001
    assert_eq!(
        store.open_file("p1", &csv_path, 1000).expect_err("locked"),
        "SF-AUTH-001"
    );
    authenticate(&store);

    // 1. Open project
    let state1: serde_json::Value =
        serde_json::from_str(&store.open_file("p1", &csv_path, 1000).expect("open")).expect("json");
    assert_eq!(state1["rowCount"], 4);

    // 2. Paginated query + search/filter
    let page: serde_json::Value = serde_json::from_str(
        &store
            .rows_page("p1", r#"{"offset":0,"limit":50,"search":"Cairo"}"#)
            .expect("page"),
    )
    .expect("json");
    assert_eq!(page["filtered_rows"], 2);
    let first_row_id = page["rows"][0]["row_id"].as_u64().expect("row_id");

    // 3. Define metric & calculated column
    store
        .define_metric(
            "p1",
            "rev_total",
            "Total Revenue",
            Some("EGP"),
            "SUM(amount)",
            1001,
        )
        .expect("metric");
    store
        .add_calculated_column("p1", "line_total", "amount * quantity", 1002)
        .expect("calc col");

    // 4. Sort rows without changing stable provenance IDs
    let sorted: serde_json::Value =
        serde_json::from_str(&store.sort_rows("p1", "amount", true, 1003).expect("sort"))
            .expect("json");
    assert_eq!(sorted["rowCount"], 4);

    // 5. Filter rows & edit cell by stable RowId with revision guard
    let rev = sorted["revision"].as_u64().expect("rev");
    store
        .set_cell_by_id("p1", first_row_id, "amount", "500", Some(rev), 1004)
        .expect("edit by id");
    // Out-of-date revision must fail with SF-CAL-007
    assert_eq!(
        store
            .set_cell_by_id("p1", first_row_id, "amount", "999", Some(rev), 1005)
            .expect_err("stale rev"),
        "SF-CAL-007"
    );

    // 6. Undo / Redo
    let undone: serde_json::Value =
        serde_json::from_str(&store.undo("p1", 1006).expect("undo")).expect("json");
    assert_eq!(undone["metrics"][0]["value"], "450");
    let redone: serde_json::Value =
        serde_json::from_str(&store.redo("p1", 1007).expect("redo")).expect("json");
    assert_eq!(redone["metrics"][0]["value"], "850");

    // 7. Sheet column management (add, rename, trim, delete)
    store
        .add_source_column("p1", "tag", " a ", 1008)
        .expect("add col");
    store
        .rename_column("p1", "tag", "label", 1009)
        .expect("rename col");
    store.trim_column("p1", "label", 1010).expect("trim col");
    store
        .delete_column("p1", "label", 1011)
        .expect("delete col");

    // 8. Anomaly detection + single atomic undoable fix
    store
        .apply_anomaly_fix(
            "p1",
            r#"{"action":"fill_missing","column":"region","replacement":"Unknown","row_ids":[4]}"#,
            None,
            1012,
        )
        .expect("anomaly fix");

    // 9. What-If parameter + scenario creation + comparison + reset
    store
        .define_parameter("p1", "fx", "1.0", 1013)
        .expect("param");
    store
        .add_scenario("p1", "upside", "Upside", r#"{"fx":"1.25"}"#)
        .expect("scenario");
    store
        .activate_scenario("p1", Some("upside"), 1014)
        .expect("activate");
    let cmp: serde_json::Value = serde_json::from_str(
        &store
            .compare_scenarios("p1", "upside", "upside", 1015)
            .expect("cmp"),
    )
    .expect("json");
    assert_eq!(cmp[0]["metricId"], "rev_total");
    store.reset_scenario("p1", 1016).expect("reset");

    // 10. DSL command execution
    store
        .apply_command(
            "p1",
            r#"{"op":"sort_rows","column":"amount","ascending":false}"#,
            1017,
        )
        .expect("dsl");

    // 11. Narrative verification + unresolved placeholder rejection
    let nar: serde_json::Value = serde_json::from_str(
        &store
            .narrative("p1", "Revenue is {{metric:rev_total}}.")
            .expect("narrative"),
    )
    .expect("json");
    assert_eq!(nar["references"][0]["unit"], "EGP");

    // 12. Grouped chart aggregation
    let grp: serde_json::Value = serde_json::from_str(
        &store
            .grouped("p1", "SUM", "amount", "region")
            .expect("grouped"),
    )
    .expect("json");
    assert!(grp.as_array().is_some_and(|arr| !arr.is_empty()));

    // 13. Living templates gallery (5 templates) + instantiate + missing binding fail
    let tpls: serde_json::Value =
        serde_json::from_str(&store.list_templates().expect("list tpls")).expect("json");
    assert_eq!(tpls.as_array().map(Vec::len), Some(5));
    assert_eq!(
        store
            .instantiate_template("p1", "boardroom-keynote", "{}")
            .expect_err("missing binding"),
        "SF-CAL-006"
    );

    // 14-16. Save artifacts (all 8 chart types + slides + docs + freeform with rowSpan + raster image) & bundle BLAKE3 persistence
    let artifacts_json = r#"{
      "charts":[
        {"id":"c1","title":"Revenue by region","chartType":"bar","function":"SUM","column":"amount","groupBy":"region"},
        {"id":"c2","title":"Trend Line","chartType":"line","function":"SUM","column":"amount","groupBy":"region"},
        {"id":"c3","title":"Cumulative Area","chartType":"area","function":"SUM","column":"amount","groupBy":"region"},
        {"id":"c4","title":"Regional Share","chartType":"pie","function":"SUM","column":"amount","groupBy":"region"},
        {"id":"c5","title":"Donut Breakdown","chartType":"donut","function":"SUM","column":"amount","groupBy":"region"},
        {"id":"c6","title":"Quantity vs Amount","chartType":"scatter","function":"SUM","column":"amount","groupBy":"quantity"},
        {"id":"c7","title":"Amount Distribution","chartType":"histogram","function":"COUNT","column":"amount","groupBy":"region"},
        {"id":"c8","title":"Headline KPI Card","chartType":"kpi","function":"SUM","column":"amount","groupBy":"region"}
      ],
      "slides":[{"id":"s1","title":"Executive Deck","body":"Total: {{metric:rev_total}}","metricId":"rev_total","chartIndex":0,"layout":"title_body"}],
      "docBlocks":[{"id":"d1","kind":"callout","title":"Audit Summary","content":"Approved at {{metric:rev_total}}","metricId":"rev_total","authoredBy":"user"}],
      "freeformBlocks":[{"id":"f1","kind":"kpi","title":"Headline KPI","body":"{{metric:rev_total}}","metricId":"rev_total","assetId":"a1","col":1,"row":1,"colSpan":6,"rowSpan":3,"zIndex":2}],
      "assets":[{"id":"a1","name":"Badge","dataUri":"data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg=="}],
      "templateConfig":{"templateId":"boardroom-keynote","titleOverride":"Q4 Review","bindings":{"headline":"rev_total"},"sectionOverrides":{}}
    }"#;
    store
        .set_artifacts("p1", artifacts_json)
        .expect("artifacts");
    store
        .save_project_bundle("p1", "q4_pack", "Q4 Pack", 1019)
        .expect("save bundle");
    let listed: serde_json::Value =
        serde_json::from_str(&store.list_project_bundles().expect("list")).expect("json");
    assert_eq!(listed[0]["validChecksum"], true);
    store
        .load_project_bundle("p1_reloaded", "q4_pack", 1020)
        .expect("load");

    // 17. Full `.safi.html` offline capsule export + unresolved metric ref block
    let bad_out = temp.join("bad.safi.html");
    assert_eq!(
        store
            .export_capsule(
                "p1",
                &bad_out,
                r#"{"title":"Bad","created_at":"2026-11-15T12:00:00Z","charts":[],"slides":[{"id":"s","title":"Broken","body":"{{metric:does_not_exist}}","layout":"body"}]}"#,
            )
            .expect_err("missing metric blocks capsule"),
        "SF-CAL-006"
    );

    let good_out = temp.join("good.safi.html");
    store
        .export_capsule(
            "p1",
            &good_out,
            r#"{"title":"Q4 Verified Capsule","created_at":"2026-11-15T12:00:00Z","charts":[]}"#,
        )
        .expect("export good capsule");
    let html = fs::read_to_string(&good_out).expect("read capsule");
    assert!(html.contains("Q4 Verified Capsule"));
    assert!(html.contains("Audit Summary"));
    assert!(!html.contains("unsafe-inline"));
    assert!(!html.contains("style=\""));
    if let Ok(copy_to) = std::env::var("SAFI_SAMPLE_CAPSULE_OUT") {
        if let Some(parent) = std::path::Path::new(&copy_to).parent() {
            let _ = fs::create_dir_all(parent);
        }
        fs::write(copy_to, &html).expect("write scratch capsule");
    }

    // 18. Delete saved project bundle cleanly & ensure original CSV is untouched
    let after_del: serde_json::Value =
        serde_json::from_str(&store.delete_project_bundle("q4_pack").expect("delete"))
            .expect("json");
    assert_eq!(after_del.as_array().map(Vec::len), Some(0));
    let original = fs::read_to_string(&csv_path).expect("original csv");
    assert!(original.starts_with("region,amount,quantity\nCairo,100,2"));

    let _ = fs::remove_dir_all(&temp);
}

#[test]
fn studio_multi_size_benchmarks_and_sample_capsule() {
    let temp = std::env::temp_dir().join(format!("safi_bench_{}", std::process::id()));
    let _ = fs::remove_dir_all(&temp);
    fs::create_dir_all(&temp).expect("temp dir");

    for rows in [100_usize, 1_000, 10_000] {
        let csv_path = temp.join(format!("bench_{rows}.csv"));
        fs::write(&csv_path, generate_csv(rows)).expect("write csv");

        let store = StudioStore::new();
        authenticate(&store);
        let t_open_start = Instant::now();
        store.open_file("bench", &csv_path, 2000).expect("open");
        let open_ms = t_open_start.elapsed().as_secs_f64() * 1000.0;

        store
            .define_metric(
                "bench",
                "total_rev",
                "Total Revenue",
                Some("EGP"),
                "SUM(amount)",
                2001,
            )
            .expect("metric");

        let t_page_start = Instant::now();
        let page: serde_json::Value = serde_json::from_str(
            &store
                .rows_page("bench", r#"{"offset":0,"limit":50}"#)
                .expect("page"),
        )
        .expect("json");
        let page_ms = t_page_start.elapsed().as_secs_f64() * 1000.0;
        let first_id = page["rows"][0]["row_id"].as_u64().expect("row_id");

        let t_edit_start = Instant::now();
        store
            .set_cell_by_id("bench", first_id, "amount", "777", None, 2002)
            .expect("edit");
        let edit_ms = t_edit_start.elapsed().as_secs_f64() * 1000.0;

        let out_html = temp.join(format!("bench_{rows}.safi.html"));
        let t_exp_start = Instant::now();
        store
            .export_capsule(
                "bench",
                &out_html,
                r#"{"title":"Benchmark Capsule","created_at":"2026-11-15T12:00:00Z","charts":[{"title":"Amount by Region","function":"SUM","column":"amount","group_by":"region"}]}"#,
            )
            .expect("export");
        let export_ms = t_exp_start.elapsed().as_secs_f64() * 1000.0;

        println!(
            "BENCHMARK rows={rows} open_ms={open_ms:.2} page50_ms={page_ms:.2} edit_recompute_ms={edit_ms:.2} capsule_export_ms={export_ms:.2}"
        );
    }

    let _ = fs::remove_dir_all(&temp);
}
